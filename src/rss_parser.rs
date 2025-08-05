use quick_xml::events::*;
use quick_xml::reader::*;
use tokio::fs::File;
use tokio::io::BufReader;
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

pub struct RssParser<T> {
    reader: Reader<BufReader<File>>,
    _phantom: std::marker::PhantomData<T>,
}

impl<T: GradualRssItem> RssParser<T> {
    pub async fn new(path: &String) -> std::io::Result<Self> {
        let file = File::open(path).await?;
        let buffer = BufReader::new(file);
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

impl <T: GradualRssItem + Unpin> Stream for RssParser<T> {
    type Item = T;

    fn poll_next(self: std::pin::Pin<&mut Self>, cx: &mut std::task::Context<'_>) -> std::task::Poll<Option<Self::Item>> {
        let fut = self.get_mut().next();
        let mut pinned_future = Box::pin(fut);
        pinned_future.as_mut().poll(cx)
    }
}
