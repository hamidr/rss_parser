use quick_xml::events::*;
use quick_xml::reader::*;
use tokio::fs::File;
use tokio::io::AsyncRead;
use tokio::io::BufReader;
use tokio::net::TcpStream;
use tokio_stream::Stream;

pub struct XmlNode {
    pub tag: String,
    pub value: Option<String>,
    pub cdata: Option<String>,
}

impl XmlNode {
    fn new(tag: String) -> Self {
        XmlNode {
            tag,
            value: None,
            cdata: None,
        }
    }
}

const XML_KEY_ITEM: &str = "item";

pub trait GradualRssItem {
    fn init() -> Self;
    fn populate(&mut self, node: XmlNode);
}

pub struct RssParser<T, R> {
    reader: Reader<BufReader<R>>,
    _phantom: std::marker::PhantomData<T>,
}

impl<T: GradualRssItem, R: AsyncRead + Unpin> RssParser<T, R> {
    pub async fn new(input: R) -> std::io::Result<Self> {
        let buffer = BufReader::new(input);
        let reader = Reader::from_reader(buffer);
        let obj = RssParser {
            reader,
            _phantom: std::marker::PhantomData,
        };
        Ok(obj)
    }

    pub async fn next(&mut self) -> Option<T> {
        let mut node_stacks: Vec<XmlNode> = Vec::new();
        let mut processing: Option<T> = None;
        let mut buf = Vec::new();

        while let Ok(event) = self.reader.read_event_into_async(&mut buf).await {
            match event {
                Event::Start(name) => {
                    let tag = String::from_utf8_lossy(name.as_ref()).to_lowercase();
                    if tag == XML_KEY_ITEM {
                        processing = Some(T::init());
                    }

                    node_stacks.push(XmlNode::new(tag));
                }
                Event::End(name) => {
                    let tag = String::from_utf8_lossy(name.as_ref()).to_lowercase();
                    if tag == XML_KEY_ITEM {
                        break;
                    }
                    if let (Some(node), Some(raw_item)) = (node_stacks.pop(), processing.as_mut()) {
                        raw_item.populate(node);
                    }
                }
                Event::CData(content) => {
                    if let Some(item) = node_stacks.last_mut() {
                        item.cdata = content.decode().ok().map(|s| s.into_owned());
                    }
                }
                Event::Text(cmt) => {
                    if let Some(item) = node_stacks.last_mut() {
                        item.value = cmt.decode().ok().map(|s| s.into_owned())
                    }
                }
                Event::Eof => break,
                _ => {}
            }
        }
        processing
    }
}

impl <T: GradualRssItem + Unpin, R: AsyncRead + Unpin> Stream for RssParser<T, R> {
    type Item = T;

    fn poll_next(self: std::pin::Pin<&mut Self>, cx: &mut std::task::Context<'_>) -> std::task::Poll<Option<Self::Item>> {
        let fut = self.get_mut().next();
        let mut pinned_future = Box::pin(fut);
        pinned_future.as_mut().poll(cx)
    }
}

// Convenience constructors for common use cases
impl<T: GradualRssItem> RssParser<T, File> {
    pub async fn from_file(path: &str) -> std::io::Result<Self> {
        let file = File::open(path).await?;
        Self::new(file).await
    }
}

impl<T: GradualRssItem> RssParser<T, TcpStream> {
    pub async fn from_tcp(stream: TcpStream) -> std::io::Result<Self> {
        Self::new(stream).await
    }
}


#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    // Test RSS item implementation
    #[derive(Debug, PartialEq)]
    struct TestRssItem {
        title: Option<String>,
        description: Option<String>,
        link: Option<String>,
        pub_date: Option<String>,
    }

    impl GradualRssItem for TestRssItem {
        fn init() -> Self {
            TestRssItem {
                title: None,
                description: None,
                link: None,
                pub_date: None,
            }
        }

        fn populate(&mut self, node: XmlNode) {
            match node.tag.as_str() {
                "title" => self.title = node.value.or(node.cdata),
                "description" => self.description = node.value.or(node.cdata),
                "link" => self.link = node.value.or(node.cdata),
                "pubdate" => self.pub_date = node.value.or(node.cdata),
                _ => {}
            }
        }
    }

    const SAMPLE_RSS: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<rss version="2.0">
    <channel>
        <title>Test RSS Feed</title>
        <description>A test RSS feed</description>
        <item>
            <title>First Item</title>
            <description>Description of first item</description>
            <link>https://example.com/1</link>
            <pubDate>Mon, 01 Jan 2024 00:00:00 GMT</pubDate>
        </item>
        <item>
            <title>Second Item</title>
            <description><![CDATA[Description with <b>HTML</b> content]]></description>
            <link>https://example.com/2</link>
            <pubDate>Tue, 02 Jan 2024 00:00:00 GMT</pubDate>
        </item>
    </channel>
</rss>"#;

    const EMPTY_RSS: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<rss version="2.0">
    <channel>
        <title>Empty RSS Feed</title>
        <description>An empty RSS feed</description>
    </channel>
</rss>"#;

    #[tokio::test]
    async fn test_parse_single_item() {
        let cursor = Cursor::new(SAMPLE_RSS.as_bytes());
        let mut parser = RssParser::<TestRssItem, _>::new(cursor).await.unwrap();

        let item = parser.next().await;
        assert!(item.is_some());

        let item = item.unwrap();
        assert_eq!(item.title, Some("First Item".to_string()));
        assert_eq!(item.description, Some("Description of first item".to_string()));
        assert_eq!(item.link, Some("https://example.com/1".to_string()));
        assert_eq!(item.pub_date, Some("Mon, 01 Jan 2024 00:00:00 GMT".to_string()));
    }

    #[tokio::test]
    async fn test_parse_multiple_items() {
        let cursor = Cursor::new(SAMPLE_RSS.as_bytes());
        let mut parser = RssParser::<TestRssItem, _>::new(cursor).await.unwrap();

        // First item
        let item1 = parser.next().await;
        assert!(item1.is_some());
        let item1 = item1.unwrap();
        assert_eq!(item1.title, Some("First Item".to_string()));

        // Second item with CDATA
        let item2 = parser.next().await;
        assert!(item2.is_some());
        let item2 = item2.unwrap();
        assert_eq!(item2.title, Some("Second Item".to_string()));
        assert_eq!(item2.description, Some("Description with <b>HTML</b> content".to_string()));

        // No more items
        let item3 = parser.next().await;
        assert!(item3.is_none());
    }

    #[tokio::test]
    async fn test_empty_rss() {
        let cursor = Cursor::new(EMPTY_RSS.as_bytes());
        let mut parser = RssParser::<TestRssItem, _>::new(cursor).await.unwrap();

        let item = parser.next().await;
        assert!(item.is_none());
    }

    #[tokio::test]
    async fn test_invalid_xml() {
        let invalid_xml = "not xml at all";
        let cursor = Cursor::new(invalid_xml.as_bytes());
        let mut parser = RssParser::<TestRssItem, _>::new(cursor).await.unwrap();

        let item = parser.next().await;
        assert!(item.is_none());
    }

    #[tokio::test]
    async fn test_stream_implementation() {
        use tokio_stream::StreamExt;

        let cursor = Cursor::new(SAMPLE_RSS.as_bytes());
        let parser = RssParser::<TestRssItem, _>::new(cursor).await.unwrap();

        let items: Vec<TestRssItem> = parser.collect().await;
        assert_eq!(items.len(), 2);
        assert_eq!(items[0].title, Some("First Item".to_string()));
        assert_eq!(items[1].title, Some("Second Item".to_string()));
    }

    #[tokio::test]
    async fn test_from_file_convenience() {
        use std::io::Write;
        use tempfile::NamedTempFile;

        // Create a temporary file with RSS content
        let mut temp_file = NamedTempFile::new().unwrap();
        write!(temp_file, "{}", SAMPLE_RSS).unwrap();
        temp_file.flush().unwrap();

        let parser = RssParser::<TestRssItem, _>::from_file(
            temp_file.path().to_str().unwrap()
        ).await;

        assert!(parser.is_ok());
        let mut parser = parser.unwrap();

        let item = parser.next().await;
        assert!(item.is_some());
        assert_eq!(item.unwrap().title, Some("First Item".to_string()));
    }

    #[tokio::test]
    async fn test_from_file_nonexistent() {
        let result = RssParser::<TestRssItem, _>::from_file("/nonexistent/file.xml").await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_xml_node_creation() {
        let node = XmlNode::new("test".to_string());
        assert_eq!(node.tag, "test");
        assert!(node.value.is_none());
        assert!(node.cdata.is_none());
    }

    #[tokio::test]
    async fn test_mixed_content() {
        let mixed_rss = r#"<?xml version="1.0" encoding="UTF-8"?>
<rss version="2.0">
    <channel>
        <item>
            <title>Mixed Content</title>
            <description>Regular text</description>
            <content><![CDATA[CDATA content]]></content>
        </item>
    </channel>
</rss>"#;

        let cursor = Cursor::new(mixed_rss.as_bytes());
        let mut parser = RssParser::<TestRssItem, _>::new(cursor).await.unwrap();

        let item = parser.next().await.unwrap();
        assert_eq!(item.title, Some("Mixed Content".to_string()));
        assert_eq!(item.description, Some("Regular text".to_string()));
    }

    #[tokio::test]
    async fn test_case_insensitive_tags() {
        let case_rss = r#"<?xml version="1.0" encoding="UTF-8"?>
<RSS version="2.0">
    <CHANNEL>
        <ITEM>
            <TITLE>Case Test</TITLE>
            <DESCRIPTION>Case insensitive tags</DESCRIPTION>
        </ITEM>
    </CHANNEL>
</RSS>"#;

        let cursor = Cursor::new(case_rss.as_bytes());
        let mut parser = RssParser::<TestRssItem, _>::new(cursor).await.unwrap();

        let item = parser.next().await.unwrap();
        assert_eq!(item.title, Some("Case Test".to_string()));
        assert_eq!(item.description, Some("Case insensitive tags".to_string()));
    }

    // Test with different AsyncRead implementations
    #[tokio::test]
    async fn test_different_async_read_types() {
        // Test with Cursor
        let cursor = Cursor::new(SAMPLE_RSS.as_bytes());
        let mut parser = RssParser::<TestRssItem, _>::new(cursor).await.unwrap();
        assert!(parser.next().await.is_some());

        // Test with empty cursor
        let empty_cursor = Cursor::new(Vec::<u8>::new());
        let mut empty_parser = RssParser::<TestRssItem, _>::new(empty_cursor).await.unwrap();
        assert!(empty_parser.next().await.is_none());
    }

    // Benchmark-style test for performance
    #[tokio::test]
    async fn test_large_rss_feed() {
        let mut large_rss = String::from(r#"<?xml version="1.0" encoding="UTF-8"?>
<rss version="2.0">
    <channel>
        <title>Large Feed</title>"#);

        // Generate 100 items
        for i in 0..100 {
            large_rss.push_str(&format!(r#"
        <item>
            <title>Item {}</title>
            <description>Description for item {}</description>
            <link>https://example.com/{}</link>
        </item>"#, i, i, i));
        }

        large_rss.push_str(r#"
    </channel>
</rss>"#);

        let cursor = Cursor::new(large_rss.as_bytes());
        let parser = RssParser::<TestRssItem, _>::new(cursor).await.unwrap();

        use tokio_stream::StreamExt;
        let items: Vec<TestRssItem> = parser.collect().await;
        assert_eq!(items.len(), 100);
        assert_eq!(items[0].title, Some("Item 0".to_string()));
        assert_eq!(items[99].title, Some("Item 99".to_string()));
    }
}