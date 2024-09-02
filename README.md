
# wikipedia-rs-wasm

[![Crates.io](https://img.shields.io/crates/v/wikipedia-wasm.svg)](https://crates.io/crates/wikipedia-wasm "Package's crates.io page")

Access wikipedia articles from Rust.

This is an alteration of the crate `wikipedia` to make it able to compile to 
wasm and should still work for other platforms. 

The only downsides are anything returning an iterator actually returns
a custom iterator that can use an async next which means no for loops and 
that your program will be all async but if it is going to be in wasm it probably
was going to be anyway.

Also, the four tests that this crate fails the original crate also fails.

# Example

```rust
use wikipedia_wasm::{Wikipedia, http};

#[tokio::main]
async fn main()
{
    let wiki = Wikipedia::<http::default::Client>::default();
    let page = wiki.page_from_title("World War II".to_string());
    let content = page.get_content().await.unwrap();
    assert!(content.starts_with("World War II or the Second World War (1 September 1939 – 2 September 1945)"));
}
```

# Problem

The original crate used [reqwest's](https://crates.io/crates/reqwest) blocking feature
which cannot compile to wasm because of the internet's single thread nature or something
like that.

# Solution

- Make most functions async
- Make an async iterator

# Original Crate by [seppo0010](https://github.com/seppo0010)
- [Github](https://github.com/seppo0010/wikipedia-rs/) - https://github.com/seppo0010/wikipedia-rs
- [Crates.io](https://crates.io/crates/wikipedia) - https://crates.io/crates/wikipedia
