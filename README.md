# RSS Parser

A high-performance, generic RSS parser for Rust that supports streaming parsing from any async input source including files, TCP streams, HTTP responses, and more.

## Features

- 🚀 **Generic AsyncRead Support**: Parse RSS from files, TCP streams, HTTP responses, or any `AsyncRead` source
- 🔄 **Streaming Interface**: Implements `tokio_stream::Stream` for memory-efficient processing of large feeds
- 📊 **Gradual Parsing**: Process RSS items one at a time without loading entire feed into memory  
- 🏷️ **CDATA Support**: Handles both regular text content and CDATA sections
- 🔤 **Case Insensitive**: Robust parsing of RSS feeds with inconsistent tag casing
- ⚡ **Async/Await**: Built on tokio for high-performance async I/O
- 🛡️ **Type Safe**: Leverage Rust's type system with custom RSS item structures

## Quick Start

Add this to your `Cargo.toml`:

```toml
[dependencies]
rss-parser = "0.1.0"
tokio = { version = "1.0", features = ["full"] }
quick-xml = "0.31"
tokio-stream = "0.1"
```

## Usage

### Define Your RSS Item Structure

```rust
use rss_parser::{GradualRssItem, XmlNode};

#[derive(Debug)]
struct Article {
    title: Option<String>,
    description: Option<String>,
    link: Option<String>,
    pub_date: Option<String>,
}

impl GradualRssItem for Article {
    fn init() -> Self {
        Article {
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
```

### Parse from File

```rust
use rss_parser::RssParser;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut parser = RssParser::<Article, _>::from_file("feed.xml").await?;
    
    while let Some(article) = parser.next().await {
        println!("Title: {:?}", article.title);
        println!("Link: {:?}", article.link);
    }
    
    Ok(())
}
```

### Parse from HTTP Response

```rust
use reqwest;
use rss_parser::RssParser;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let response = reqwest::get("https://example.com/feed.xml").await?;
    let stream = response.bytes_stream();
    
    // Convert bytes stream to AsyncRead
    let reader = tokio_util::io::StreamReader::new(
        stream.map(|result| result.map_err(std::io::Error::other))
    );
    
    let mut parser = RssParser::<Article, _>::new(reader).await?;
    
    while let Some(article) = parser.next().await {
        println!("Article: {:?}", article);
    }
    
    Ok(())
}
```

### Parse from TCP Stream

```rust
use tokio::net::TcpStream;
use rss_parser::RssParser;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let stream = TcpStream::connect("example.com:80").await?;
    let parser = RssParser::<Article, _>::from_tcp(stream).await?;
    
    // Use as stream
    use tokio_stream::StreamExt;
    let articles: Vec<Article> = parser.collect().await;
    
    println!("Parsed {} articles", articles.len());
    Ok(())
}
```

### Using as a Stream

```rust
use tokio_stream::StreamExt;
use rss_parser::RssParser;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let parser = RssParser::<Article, _>::from_file("feed.xml").await?;
    
    // Process articles as they're parsed
    parser
        .for_each(|article| async move {
            println!("Processing: {:?}", article.title);
            // Process article...
        })
        .await;
    
    Ok(())
}
```

## Advanced Usage

### Custom Input Sources

The parser accepts any type implementing `AsyncRead + Unpin`:

```rust
use std::io::Cursor;
use rss_parser::RssParser;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let rss_data = r#"<?xml version="1.0"?>
    <rss version="2.0">
        <channel>
            <item>
                <title>Example Article</title>
                <description>This is an example</description>
            </item>
        </channel>
    </rss>"#;
    
    let cursor = Cursor::new(rss_data.as_bytes());
    let mut parser = RssParser::<Article, _>::new(cursor).await?;
    
    if let Some(article) = parser.next().await {
        println!("Parsed: {:?}", article);
    }
    
    Ok(())
}
```

### Filtering and Processing

```rust
use tokio_stream::StreamExt;

let parser = RssParser::<Article, _>::from_file("feed.xml").await?;

let recent_articles: Vec<Article> = parser
    .filter(|article| {
        // Filter articles based on some criteria
        article.title.as_ref().map_or(false, |title| title.contains("Rust"))
    })
    .take(10)  // Take only first 10 matching articles
    .collect()
    .await;
```

## API Reference

### `RssParser<T, R>`

The main parser struct, generic over:
- `T`: Your RSS item type implementing `GradualRssItem`
- `R`: The input source implementing `AsyncRead + Unpin`

#### Methods

- `new(input: R) -> Result<Self, std::io::Error>`: Create parser from any AsyncRead source
- `from_file(path: &str) -> Result<Self, std::io::Error>`: Convenience constructor for files  
- `from_tcp(stream: TcpStream) -> Result<Self, std::io::Error>`: Convenience constructor for TCP streams
- `next(&mut self) -> Option<T>`: Parse and return the next RSS item
- Implements `Stream<Item = T>` for use with `tokio-stream`

### `GradualRssItem` Trait

Implement this trait for your RSS item structures:

```rust
pub trait GradualRssItem {
    fn init() -> Self;
    fn populate(&mut self, node: XmlNode);
}
```

### `XmlNode`

Represents a parsed XML node:

```rust
pub struct XmlNode {
    pub tag: String,        // The XML tag name (lowercase)
    pub value: Option<String>,   // Text content
    pub cdata: Option<String>,   // CDATA content
}
```

## Performance

The parser is designed for high performance and low memory usage:

- **Streaming**: Processes RSS items one at a time, not loading entire feed into memory
- **Zero-copy**: Minimizes string allocations where possible
- **Async**: Non-blocking I/O for handling multiple feeds concurrently

## Error Handling

The parser uses Rust's standard error handling patterns:

- Constructor methods return `Result<RssParser<T, R>, std::io::Error>`
- `next()` returns `Option<T>` - `None` indicates end of feed or parse error
- Malformed XML is handled gracefully, skipping problematic sections when possible

## Requirements

- Rust 1.75+
- tokio runtime
- `quick-xml` for XML parsing
- `tokio-stream` for Stream implementation

## Contributing

Contributions are welcome! Please feel free to submit a Pull Request. For major changes, please open an issue first to discuss what you would like to change.

### Running Tests

```bash
cargo test
```

### Running Examples

```bash
cargo run --example basic_usage
cargo run --example http_parsing  
cargo run --example stream_processing
```

## License

This project is licensed under the MIT License - see the [LICENSE](LICENSE) file for details.

## Changelog

### v0.1.0
- Initial release
- Generic AsyncRead support
- Stream trait implementation
- File and TCP convenience constructors
- CDATA support
- Case-insensitive parsing

## Related Projects

- [quick-xml](https://github.com/tafia/quick-xml) - Fast XML parser used internally
- [tokio](https://tokio.rs/) - Async runtime
- [feed-rs](https://github.com/feed-rs/feed-rs) - Alternative feed parser with more format support

---

**Note**: This parser is specifically designed for RSS feeds. For Atom feeds or other syndication formats, consider using a more comprehensive feed parsing library.
