//! Access wikipedia articles from Rust.
//!
//! # Example
//!
//! ```rust
//! use wikipedia_wasm::{Wikipedia, http};
//!
//! #[tokio::main]
//! async fn main()
//! {
//!     let wiki = Wikipedia::<http::default::Client>::default();
//!     let page = wiki.page_from_title("World War II".to_string());
//!     let content = page.get_content().await.unwrap();
//!     assert!(content.starts_with("World War II or the Second World War (1 September 1939 – 2 September 1945)"));
//! }
//! ```
#[cfg(feature="http-client")] extern crate reqwest;
#[cfg(feature="http-client")] extern crate url;
extern crate serde_json;
#[macro_use] extern crate failure;

use std::cmp::PartialEq;
use std::io;
use std::result;

pub mod iter;
pub mod http;
pub use iter::Iter;

const LANGUAGE_URL_MARKER:&'static str = "{language}";

macro_rules! results {
    ($data: expr, $query_field: expr) => {
        // There has to be a better way to write the following code
        $data.as_object()
        .and_then(|x| x.get("query"))
        .and_then(|x| x.as_object())
        .and_then(|x| x.get($query_field))
        .and_then(|x| x.as_array())
        .ok_or(Error::JSONPathError)?
            .into_iter().filter_map(|i|
                i.as_object()
                .and_then(|i| i.get("title"))
                .and_then(|s| s.as_str().map(|s| s.to_owned()))
                ).collect()
    }
}

macro_rules! cont {
    ($this: expr, $cont: expr, $($params: expr),*) => { async {
        let qp = $this.identifier.query_param();
        let mut params = vec![
            $($params),*,
            ("format", "json"),
            ("action", "query"),
            (&*qp.0, &*qp.1),
        ];
        match *$cont {
            Some(ref v) => {
                for x in v.iter() { params.push((&*x.0, &*x.1)); }
            },
            None => params.push(("continue", "")),
        }
        let q = $this.wikipedia.query(params.into_iter()).await?;

        let pages = q
            .as_object()
            .and_then(|x| x.get("query"))
            .and_then(|x| x.as_object())
            .and_then(|x| x.get("pages"))
            .and_then(|x| x.as_object())
            .ok_or(Error::JSONPathError)?;

        Ok((pages.values().cloned().collect(), $this.parse_cont(&q)?))
    } }
}

/// Wikipedia failed to fetch some information
#[derive(Fail, Debug)]
pub enum Error {
    /// Some error communicating with the server
    #[fail(display = "HTTP Error")]
    HTTPError,
    /// Error reading response
    #[fail(display = "IO Error: {}", _0)]
    IOError(#[cause] io::Error),
    /// Failed to parse JSON response
    #[fail(display = "JSON Error: {}", _0)]
    JSONError(#[cause] serde_json::error::Error),
    /// Missing required keys in the JSON response
    #[fail(display = "JSON Path Error")]
    JSONPathError,
    /// One of the parameters provided (identified by `String`) is invalid
    #[fail(display = "Invalid Parameter: {}", _0)]
    InvalidParameter(String),
}

pub type Result<T> = result::Result<T, Error>;

#[derive(Debug)]
pub struct Wikipedia<A: http::HttpClient> {
    /// HttpClient struct.
    pub client: A,
    /// Url is created by concatenating `pre_language_url` + `language` + `post_language_url`.
    pub pre_language_url: String,
    pub post_language_url: String,
    pub language: String,
    /// Number of results to fetch when searching.
    pub search_results: u32,
    /// Number of images to fetch in each request when calling `get_images`.
    /// The iterator will go through all of them, fetching pages of this size.
    /// It can be the string "max" to fetch as many as possible on every request.
    pub images_results: String,
    /// Like `images_results`, for links and references.
    pub links_results: String,
    /// Like `images_results`, for categories.
    pub categories_results: String,
}

impl<A: http::HttpClient + Default> Default for Wikipedia<A> {
    fn default() -> Self {
        Wikipedia::new(A::default())
    }
}

impl<A: http::HttpClient + Clone> Clone for Wikipedia<A> {
    fn clone(&self) -> Self {
        Wikipedia {
            client: self.client.clone(),
            pre_language_url: self.pre_language_url.clone(),
            post_language_url: self.post_language_url.clone(),
            language: self.language.clone(),
            search_results: self.search_results.clone(),
            images_results: self.images_results.clone(),
            links_results: self.links_results.clone(),
            categories_results: self.categories_results.clone(),
        }
    }
}

impl<A: http::HttpClient> Wikipedia<A> {
    /// Creates a new object using the provided client and default values.
    pub fn new(mut client: A) -> Self {
        client.user_agent("wikipedia (https://github.com/seppo0010/wikipedia-rs)".to_owned());
        Wikipedia {
            client: client,
            pre_language_url: "https://".to_owned(),
            post_language_url: ".wikipedia.org/w/api.php".to_owned(),
            language: "en".to_owned(),
            search_results: 10,
            images_results: "max".to_owned(),
            links_results: "max".to_owned(),
            categories_results: "max".to_owned(),
        }
    }

    /// Returns a list of languages in the form of (`identifier`, `language`),
    /// for example [("en", "English"), ("es", "Español")]
    pub async fn get_languages(&self) -> Result<Vec<(String, String)>> {
        let q = self.query(vec![
            ("meta", "siteinfo"),
            ("siprop", "languages"),
            ("format", "json"),
            ("action", "query"),
        ].into_iter()).await?;

        Ok(q
            .as_object()
            .and_then(|x| x.get("query"))
            .and_then(|x| x.as_object())
            .and_then(|x| x.get("languages"))
            .and_then(|x| x.as_array())
            .ok_or(Error::JSONPathError)?
            .into_iter()
            .filter_map(|x| {
                        let o = x.as_object();
                        Some((
                            match o
                                .and_then(|x| x.get("code"))
                                .and_then(|x| x.as_str())
                                .map(|x| x.to_owned()) {
                                    Some(v) => v,
                                    None => return None,
                                },
                            match o
                                .and_then(|x| x.get("*"))
                                .and_then(|x| x.as_str())
                                .map(|x| x.to_owned()) {
                                    Some(v) => v,
                                    None => return None,
                                },
                        ))
                    })
            .collect())
    }

    /// Returns the api url
    pub fn base_url(&self) -> String {
        format!("{}{}{}", self.pre_language_url, self.language, self.post_language_url)
    }

    /// Updates the url format. The substring `{language}` will be replaced
    /// with the selected language.
    pub fn set_base_url(&mut self, base_url: &str) {
        let index = match base_url.find(LANGUAGE_URL_MARKER) {
            Some(i) => i,
            None => {
                self.pre_language_url = base_url.to_owned();
                self.language = "".to_owned();
                self.post_language_url = "".to_owned();
                return;
            }
        };
        self.pre_language_url = base_url[0..index].to_owned();
        self.post_language_url = base_url[index+LANGUAGE_URL_MARKER.len()..].to_owned();
    }

    async fn query<'a, I>(&self, args: I) -> Result<serde_json::Value>
            where I: Iterator<Item=(&'a str, &'a str)> {
        let response_str = self.client.get(&*self.base_url(), args).await.map_err(|_| Error::HTTPError)?;
        let json = serde_json::from_str(&*response_str).map_err(Error::JSONError)?;
        Ok(json)
    }

    /// Searches for a string and returns a list of relevant page titles.
    ///
    /// # Examples
    ///
    /// ```
    /// extern crate wikipedia_wasm;
    ///
    /// let wiki = wikipedia_wasm::Wikipedia::<wikipedia_wasm::http::default::Client>::default();
    /// let results = wiki.search("keyboard").unwrap();
    /// assert!(results.contains(&"Computer keyboard".to_owned()));
    /// ```
    pub async fn search(&self, query: &str) -> Result<Vec<String>> {
        let results = &*format!("{}", self.search_results);
        let data = self.query(vec![
            ("list", "search"),
            ("srprop", ""),
            ("srlimit", results),
            ("srsearch", query),
            ("format", "json"),
            ("action", "query"),
        ].into_iter()).await?;

        Ok(results!(data, "search"))
    }

    /// Search articles within `radius` meters of `latitude` and `longitude`.
    ///
    /// # Examples
    ///
    /// ```
    /// extern crate wikipedia_wasm;
    ///
    /// let wiki = wikipedia_wasm::Wikipedia::<wikipedia_wasm::http::default::Client>::default();
    /// let results = wiki.geosearch(40.750556,-73.993611, 20).unwrap();
    /// assert!(results.contains(&"Madison Square Garden".to_owned()));
    /// ```
    pub async fn geosearch(&self, latitude: f64, longitude: f64, radius: u16) -> Result<Vec<String>> {
        if latitude < -90.0 || latitude > 90.0 {
            return Err(Error::InvalidParameter("latitude".to_string()))
        }
        if longitude < -180.0 || longitude > 180.0 {
            return Err(Error::InvalidParameter("longitude".to_string()))
        }
        if radius < 10 || radius > 10000 {
            return Err(Error::InvalidParameter("radius".to_string()))
        }
        let results = &*format!("{}", self.search_results);
        let data = self.query(vec![
            ("list", "geosearch"),
            ("gsradius", &*format!("{}", radius)),
            ("gscoord", &*format!("{}|{}", latitude, longitude)),
            ("gslimit", results),
            ("format", "json"),
            ("action", "query"),
        ].into_iter()).await?;
        Ok(results!(data, "geosearch"))
    }

    /// Fetches `count` random articles' title.
    pub async fn random_count(&self, count: u8) -> Result<Vec<String>> {
        let data = self.query(vec![
            ("list", "random"),
            ("rnnamespace", "0"),
            ("rnlimit", &*format!("{}", count)),
            ("format", "json"),
            ("action", "query"),
        ].into_iter()).await?;
        let r:Vec<String> = results!(data, "random");
        Ok(r)
    }

    /// Fetches a random article's title.
    pub async fn random(&self) -> Result<Option<String>> {
        Ok(self.random_count(1).await?.into_iter().next())
    }

    /// Creates a new `Page` given a `title`.
    pub fn page_from_title<'a>(&'a self, title: String) -> Page<'a, A> {
        Page::from_title(self, title)
    }

    /// Creates a new `Page` given a `pageid`.
    pub fn page_from_pageid<'a>(&'a self, pageid: String) -> Page<'a, A> {
        Page::from_pageid(self, pageid)
    }
}

#[derive(Debug)]
enum TitlePageId {
    Title(String),
    PageId(String),
}

impl TitlePageId {
    fn query_param(&self) -> (String, String) {
        match *self {
            TitlePageId::Title(ref s) => ("titles".to_owned(), s.clone()),
            TitlePageId::PageId(ref s) => ("pageids".to_owned(), s.clone()),
        }
    }
}

#[derive(Debug)]
pub struct Page<'a, A: 'a + http::HttpClient> {
    wikipedia: &'a Wikipedia<A>,
    identifier: TitlePageId,
}

/// A wikipedia article.
impl<'a, A: http::HttpClient> Page<'a, A> {
    /// Creates a new `Page` given a `title`.
    pub fn from_title(wikipedia: &'a Wikipedia<A>, title: String) -> Page<A> {
        Page { wikipedia: wikipedia, identifier: TitlePageId::Title(title) }
    }

    /// Creates a new `Page` given a `pageid`.
    pub fn from_pageid(wikipedia: &'a Wikipedia<A>, pageid: String) -> Page<A> {
        Page { wikipedia: wikipedia, identifier: TitlePageId::PageId(pageid) }
    }

    /// Gets the `Page`'s `pageid`.
    #[async_recursion::async_recursion(?Send)]
    pub async fn get_pageid(&self) -> Result<String> {
        match self.identifier {
            TitlePageId::PageId(ref s) => Ok(s.clone()),
            TitlePageId::Title(_) => {
                let qp = self.identifier.query_param();
                let q = self.wikipedia.query(vec![
                    ("prop", "info|pageprops"),
                    ("inprop", "url"),
                    ("ppprop", "disambiguation"),
                    ("redirects", ""),
                    ("format", "json"),
                    ("action", "query"),
                    (&*qp.0, &*qp.1),
                ].into_iter()).await?;

                match self.redirect(&q) {
                    Some(r) => return Page::from_title(&self.wikipedia, r).get_pageid().await,
                    None => (),
                }
                let pages = q
                    .as_object()
                    .and_then(|x| x.get("query"))
                    .and_then(|x| x.as_object())
                    .and_then(|x| x.get("pages"))
                    .and_then(|x| x.as_object())
                    .ok_or(Error::JSONPathError)?;
                pages.keys().cloned().next().ok_or(Error::JSONPathError)
            }
        }
    }

    /// Gets the `Page`'s `title`.
    pub async fn get_title(&self) -> Result<String> {
        match self.identifier {
            TitlePageId::Title(ref s) => Ok(s.clone()),
            TitlePageId::PageId(_) => {
                let qp = self.identifier.query_param();
                let q = self.wikipedia.query(vec![
                    ("prop", "info|pageprops"),
                    ("inprop", "url"),
                    ("ppprop", "disambiguation"),
                    ("redirects", ""),
                    ("format", "json"),
                    ("action", "query"),
                    (&*qp.0, &*qp.1),
                ].into_iter()).await?;

                match self.redirect(&q) {
                    Some(r) => return Ok(r),
                    None => (),
                }
                let pages = q
                    .as_object()
                    .and_then(|x| x.get("query"))
                    .and_then(|x| x.as_object())
                    .and_then(|x| x.get("pages"))
                    .and_then(|x| x.as_object())
                    .ok_or(Error::JSONPathError)?;
                let page = match pages.values().next() {
                    Some(p) => p,
                    None => return Err(Error::JSONPathError),
                };
                Ok(page.as_object()
                    .and_then(|x| x.get("title"))
                    .and_then(|x| x.as_str())
                    .ok_or(Error::JSONPathError)?
                    .to_owned())
            },
        }
    }

    /// If the `Page` redirects to another one it returns its title, otherwise
    /// returns None.
    fn redirect(&self, q: &serde_json::Value) -> Option<String> {
        q.as_object()
            .and_then(|x| x.get("query"))
            .and_then(|x| x.as_object())
            .and_then(|x| x.get("redirects"))
            .and_then(|x| x.as_array())
            .and_then(|x| x.into_iter().next())
            .and_then(|x| x.as_object())
            .and_then(|x| x.get("to"))
            .and_then(|x| x.as_str())
            .map(|x| x.to_owned())
    }

    /// Given a parsed response, usually we access the first page with the data
    fn get_first_page<'parsed>(&self, data: &'parsed serde_json::Value) -> Option<&'parsed serde_json::Value> {
        let pages = data
            .as_object()
            .and_then(|x| x.get("query"))
            .and_then(|x| x.as_object())
            .and_then(|x| x.get("pages"))
            .and_then(|x| x.as_object());
        let pageid = match pages {
            Some(some_pages) => match some_pages.keys().next() {
                Some(pageid) => pageid,
                None => return None,
            },
            None => return None,
        };
        pages.unwrap().get(pageid)
    }

    /// Gets the markdown content of the article.
    #[async_recursion::async_recursion(?Send)]
    pub async fn get_content(&self) -> Result<String> {
        let qp = self.identifier.query_param();
        let q = self.wikipedia.query(vec![
            ("prop", "extracts|revisions"),
            ("explaintext", ""),
            ("rvprop", "ids"),
            ("redirects", ""),
            ("format", "json"),
            ("action", "query"),
            (&*qp.0, &*qp.1),
        ].into_iter()).await?;

        match self.redirect(&q) {
            Some(r) => return Page::from_title(&self.wikipedia, r).get_content().await,
            None => (),
        };

        Ok(self.get_first_page(&q)
            .and_then(|x| x.as_object())
            .and_then(|x| x.get("extract"))
            .and_then(|x| x.as_str())
            .ok_or(Error::JSONPathError)?
            .to_owned())
    }

    /// Gets the html content of the article.
    #[async_recursion::async_recursion(?Send)]
    pub async fn get_html_content(&self) -> Result<String> {
        let qp = self.identifier.query_param();
        let q = self.wikipedia.query(vec![
            ("prop", "revisions"),
            ("rvprop", "content"),
            ("rvlimit", "1"),
            ("rvparse", ""),
            ("redirects", ""),
            ("format", "json"),
            ("action", "query"),
            (&*qp.0, &*qp.1),
        ].into_iter()).await?;

        match self.redirect(&q) {
            Some(r) => return Page::from_title(&self.wikipedia, r).get_html_content().await,
            None => (),
        }

        Ok(self.get_first_page(&q)
            .and_then(|x| x.as_object())
            .and_then(|x| x.get("revisions"))
            .and_then(|x| x.as_array())
            .and_then(|x| x.into_iter().next())
            .and_then(|x| x.as_object())
            .and_then(|x| x.get("*"))
            .and_then(|x| x.as_str())
            .ok_or(Error::JSONPathError)?
            .to_owned())
    }

    /// Gets a summary of the article.
    #[async_recursion::async_recursion(?Send)]
    pub async fn get_summary(&self) -> Result<String> {
        let qp = self.identifier.query_param();
        let q = self.wikipedia.query(vec![
            ("prop", "extracts"),
            ("explaintext", ""),
            ("exintro", ""),
            ("redirects", ""),
            ("format", "json"),
            ("action", "query"),
            (&*qp.0, &*qp.1),
        ].into_iter()).await?;

        match self.redirect(&q) {
            Some(r) => return Page::from_title(&self.wikipedia, r).get_summary().await,
            None => (),
        }

        Ok(self.get_first_page(&q)
            .and_then(|x| x.as_object())
            .and_then(|x| x.get("extract"))
            .and_then(|x| x.as_str())
            .ok_or(Error::JSONPathError)?
            .to_owned())
    }

    /// Receive a json object and extracts any `continue` parameters to be
    /// used when browsing following pages.
    fn parse_cont(&self, q: &serde_json::Value) -> Result<Option<Vec<(String, String)>>> {
        let cont = match q
            .as_object()
            .and_then(|x| x.get("continue"))
            .and_then(|x| x.as_object()) {
            Some(v) => v,
            None => return Ok(None),
        };
        let mut cont_v = vec![];
        for (k, v) in cont.into_iter() {
            let value = match *v {
                serde_json::Value::Null => "".to_owned(),
                serde_json::Value::Bool(b) => if b { "1" } else { "0" }.to_owned(),
                serde_json::Value::Number(ref f) => format!("{}", f),
                serde_json::Value::String(ref s) => s.clone(),
                _ => return Err(Error::JSONPathError),
            };
            cont_v.push((k.clone(), value));
        }
        Ok(Some(cont_v))
    }

    async fn request_images(&self, cont: &Option<Vec<(String, String)>>) ->
            Result<(Vec<serde_json::Value>, Option<Vec<(String, String)>>)> {
        cont!(self, cont,
            ("generator", "images"),
            ("gimlimit", &*self.wikipedia.images_results),
            ("prop", "imageinfo"),
            ("iiprop", "url")
        ).await
    }

    /// Creates an iterator to view all images in the `Page`.
    pub async fn get_images(&self) -> Result<Iter<'_, A, iter::Image>> {
        Iter::new(&self).await
    }

    async fn request_extlinks(&self, cont: &Option<Vec<(String, String)>>) ->
            Result<(Vec<serde_json::Value>, Option<Vec<(String, String)>>)> {
        let a:Result<(Vec<serde_json::Value>, _)> = cont!(self, cont,
            ("prop", "extlinks"),
            ("ellimit", &*self.wikipedia.links_results)
        ).await;
        a.map(|(pages, cont)| {
            let page = match pages.into_iter().next() {
                Some(p) => p,
                None => return (Vec::new(), None),
            };
            (page
                .as_object()
                .and_then(|x| x.get("extlinks"))
                .and_then(|x| x.as_array())
                .map(|x| x.into_iter().cloned().collect())
                .unwrap_or(Vec::new()), cont)
        })
    }

    /// Creates an iterator to view all references (external links) in the `Page`.
    pub async fn get_references(&self) -> Result<Iter<A, iter::Reference>> {
        Iter::new(&self).await
    }

    async fn request_links(&self, cont: &Option<Vec<(String, String)>>) ->
            Result<(Vec<serde_json::Value>, Option<Vec<(String, String)>>)> {
        let a:Result<(Vec<serde_json::Value>, _)> = cont!(self, cont,
            ("prop", "links"),
            ("plnamespace", "0"),
            ("ellimit", &*self.wikipedia.links_results)
        ).await;
        a.map(|(pages, cont)| {
            let page = match pages.into_iter().next() {
                Some(p) => p,
                None => return (Vec::new(), None),
            };
            (page
                .as_object()
                .and_then(|x| x.get("links"))
                .and_then(|x| x.as_array())
                .map(|x| x.into_iter().cloned().collect())
                .unwrap_or(Vec::new()), cont)
        })
    }

    /// Creates an iterator to view all internal links in the `Page`.
    pub async fn get_links(&self) -> Result<Iter<A, iter::Link>> {
        Iter::new(&self).await
    }

    async fn request_categories(&self, cont: &Option<Vec<(String, String)>>) ->
            Result<(Vec<serde_json::Value>, Option<Vec<(String, String)>>)> {
        let a:Result<(Vec<serde_json::Value>, _)> = cont!(self, cont,
            ("prop", "categories"),
            ("cllimit", &*self.wikipedia.categories_results)
        ).await;
        a.map(|(pages, cont)| {
            let page = match pages.into_iter().next() {
                Some(p) => p,
                None => return (Vec::new(), None),
            };
            (page
                .as_object()
                .and_then(|x| x.get("categories"))
                .and_then(|x| x.as_array())
                .map(|x| x.into_iter().cloned().collect())
                .unwrap_or(Vec::new()), cont)
        })
    }

    /// Creates an iterator to view all categories of the `Page`.
    pub async fn get_categories(&self) -> Result<Iter<A, iter::Category>> {
        Iter::new(&self).await
    }

    async fn request_langlinks(&self, cont: &Option<Vec<(String, String)>>) ->
            Result<(Vec<serde_json::Value>, Option<Vec<(String, String)>>)> {
        let a:Result<(Vec<serde_json::Value>, _)> = cont!(self, cont,
            ("prop", "langlinks"),
            ("lllimit", &*self.wikipedia.links_results)
        ).await;
        a.map(|(pages, cont)| {
            let page = match pages.into_iter().next() {
                Some(p) => p,
                None => return (Vec::new(), None),
            };
            (page
                .as_object()
                .and_then(|x| x.get("langlinks"))
                .and_then(|x| x.as_array())
                .map(|x| x.into_iter().cloned().collect())
                .unwrap_or(Vec::new()), cont)
        })
    }

    /// Creates an iterator to view all langlinks of the `Page`.
    /// This iterates over the page titles in all available languages.
    pub async fn get_langlinks(&self) -> Result<Iter<A, iter::LangLink>> {
        Iter::new(&self).await
    }

    /// Returns the latitude and longitude associated to the `Page` if any.
    #[async_recursion::async_recursion(?Send)]
    pub async fn get_coordinates(&self) -> Result<Option<(f64, f64)>> {
        let qp = self.identifier.query_param();
        let params = vec![
            ("prop", "coordinates"),
            ("colimit", "max"),
            ("redirects", ""),
            ("format", "json"),
            ("action", "query"),
            (&*qp.0, &*qp.1),
        ];
        let q = self.wikipedia.query(params.into_iter()).await?;

        match self.redirect(&q) {
            Some(r) => return Page::from_title(&self.wikipedia, r).get_coordinates().await,
            None => (),
        }

        let coord = match self.get_first_page(&q)
                .and_then(|x| x.as_object())
                .and_then(|x| x.get("coordinates"))
                .and_then(|x| x.as_array())
                .and_then(|x| x.into_iter().next())
                .and_then(|x| x.as_object()) {
            Some(c) => c,
            None => return Ok(None),
        };
        Ok(Some((
            coord.get("lat").and_then(|x| x.as_f64()).ok_or(Error::JSONPathError)?,
            coord.get("lon").and_then(|x| x.as_f64()).ok_or(Error::JSONPathError)?,
        )))
    }

    /// Fetches all sections of the article.
    pub async fn get_sections(&self) -> Result<Vec<String>> {
        let pageid = self.get_pageid().await?;
        let params = vec![
            ("prop", "sections"),
            ("format", "json"),
            ("action", "parse"),
            ("pageid", &*pageid),
        ];
        let q = self.wikipedia.query(params.into_iter()).await?;

        Ok(q
            .as_object()
            .and_then(|x| x.get("parse"))
            .and_then(|x| x.as_object())
            .and_then(|x| x.get("sections"))
            .and_then(|x| x.as_array())
            .ok_or(Error::JSONPathError)?
            .into_iter()
            .filter_map(|x| x.as_object()
                    .and_then(|x| x.get("line"))
                    .and_then(|x| x.as_str())
                    .map(|x| x.to_owned())
                    )
            .collect())
    }

    /// Fetches the content of a section.
    pub async fn get_section_content(&self, title: &str) -> Result<Option<String>> {
        let headr = format!("== {} ==", title);
        let content = self.get_content().await?;
        let index = match content.find(&*headr) {
            Some(i) => headr.len() + i,
            None => return Ok(None),
        };
        let end = match content[index..].find("==") {
            Some(i) => index + i,
            None => content.len(),
        };
        Ok(Some(content[index..end].to_owned()))
    }
}

impl<'a, A: http::HttpClient> PartialEq<Page<'a, A>> for Page<'a, A> {
    fn eq(&self, other: &Page<A>) -> bool {
        match self.identifier {
            TitlePageId::Title(ref t1) => match other.identifier {
                TitlePageId::Title(ref t2) => t1 == t2,
                TitlePageId::PageId(_) => false,
            },
            TitlePageId::PageId(ref p1) => match other.identifier {
                TitlePageId::Title(_) => false,
                TitlePageId::PageId(ref p2) => p1 == p2,
            },
        }
    }
}

#[cfg(test)]
mod test {
    use super::Wikipedia;
    use super::http::HttpClient;
    use super::iter;
    use std::sync::Mutex;

    use crate::iter::AsyncIterator;

    struct MockClient {
        pub url: Mutex<Vec<String>>,
        pub user_agent: Option<String>,
        pub arguments: Mutex<Vec<Vec<(String, String)>>>,
        pub response: Mutex<Vec<String>>,
    }

    impl Default for MockClient {
        fn default() -> Self {
            MockClient {
                url: Mutex::new(Vec::new()),
                user_agent: None,
                arguments: Mutex::new(Vec::new()),
                response: Mutex::new(Vec::new()),
            }
        }
    }

    impl super::http::HttpClient for MockClient {
        fn user_agent(&mut self, user_agent: String) {
            self.user_agent = Some(user_agent)
        }

        async fn get<'a, I>(&self, base_url: &str, args: I) -> Result<String, super::http::Error>
                where I: Iterator<Item=(&'a str, &'a str)> {
            self.url.lock().unwrap().push(base_url.to_owned());
            self.arguments.lock().unwrap().push(args.map(|x| (x.0.to_owned(), x.1.to_owned())).collect());
            Ok(self.response.lock().unwrap().remove(0))
        }
    }

    #[test]
    fn base_url() {
        let mut wikipedia = Wikipedia::<MockClient>::default();
        assert_eq!(wikipedia.base_url(), "https://en.wikipedia.org/w/api.php");
        wikipedia.language = "es".to_owned();
        assert_eq!(wikipedia.base_url(), "https://es.wikipedia.org/w/api.php");

        wikipedia.set_base_url("https://hello.{language}.world/");
        assert_eq!(wikipedia.base_url(), "https://hello.es.world/");

        wikipedia.set_base_url("https://hello.world/");
        assert_eq!(wikipedia.base_url(), "https://hello.world/");
    }

    #[tokio::test]
    async fn user_agent() {
        let mut wikipedia = Wikipedia::<MockClient>::default();
        wikipedia.client.response.lock().unwrap().push("{}".to_owned());
        wikipedia.search("hello world").await.unwrap_err();
        assert_eq!(&*wikipedia.client.user_agent.unwrap(), "wikipedia (https://github.com/seppo0010/wikipedia-rs)");

        let mut client = MockClient::default();
        client.user_agent("hello world".to_owned());
        client.response.lock().unwrap().push("{}".to_owned());
        wikipedia.client = client;
        wikipedia.search("hello world").await.unwrap_err();
        assert_eq!(&*wikipedia.client.user_agent.unwrap(), "hello world");
    }

    #[tokio::test]
    async fn search() {
        let wikipedia = Wikipedia::<MockClient>::default();
        wikipedia.client.response.lock().unwrap().push("{\"query\":{\"search\":[{\"title\":\"hello\"}, {\"title\":\"world\"}]}}".to_owned());
        assert_eq!(
                wikipedia.search("hello world").await.unwrap(),
                vec![
                "hello".to_owned(),
                "world".to_owned(),
                ]);
        assert_eq!(*wikipedia.client.url.lock().unwrap(),
                vec!["https://en.wikipedia.org/w/api.php".to_owned()]);
        assert_eq!(*wikipedia.client.arguments.lock().unwrap(),
                vec![vec![
                    ("list".to_owned(), "search".to_owned()),
                    ("srprop".to_owned(), "".to_owned()),
                    ("srlimit".to_owned(), "10".to_owned()),
                    ("srsearch".to_owned(), "hello world".to_owned()),
                    ("format".to_owned(), "json".to_owned()),
                    ("action".to_owned(), "query".to_owned())
                    ]]);
    }

    #[tokio::test]
    async fn geosearch() {
        let wikipedia = Wikipedia::<MockClient>::default();
        wikipedia.client.response.lock().unwrap().push("{\"query\":{\"geosearch\":[{\"title\":\"hello\"}, {\"title\":\"world\"}]}}".to_owned());
        assert_eq!(
                wikipedia.geosearch(-34.603333, -58.381667, 10).await.unwrap(),
                vec![
                "hello".to_owned(),
                "world".to_owned(),
                ]);
        assert_eq!(*wikipedia.client.url.lock().unwrap(),
                vec!["https://en.wikipedia.org/w/api.php".to_owned()]);
        assert_eq!(*wikipedia.client.arguments.lock().unwrap(),
                vec![vec![
                    ("list".to_owned(), "geosearch".to_owned()),
                    ("gsradius".to_owned(), "10".to_owned()),
                    ("gscoord".to_owned(), "-34.603333|-58.381667".to_owned()),
                    ("gslimit".to_owned(), "10".to_owned()),
                    ("format".to_owned(), "json".to_owned()),
                    ("action".to_owned(), "query".to_owned())
                    ]]);
    }

    #[tokio::test]
    async fn random_count() {
        let wikipedia = Wikipedia::<MockClient>::default();
        wikipedia.client.response.lock().unwrap().push("{\"query\":{\"random\":[{\"title\":\"hello\"}, {\"title\":\"world\"}]}}".to_owned());
        assert_eq!(
                wikipedia.random_count(10).await.unwrap(),
                vec![
                "hello".to_owned(),
                "world".to_owned(),
                ]);
        assert_eq!(*wikipedia.client.url.lock().unwrap(),
                vec!["https://en.wikipedia.org/w/api.php".to_owned()]);
        assert_eq!(*wikipedia.client.arguments.lock().unwrap(),
                vec![vec![
                    ("list".to_owned(), "random".to_owned()),
                    ("rnnamespace".to_owned(), "0".to_owned()),
                    ("rnlimit".to_owned(), "10".to_owned()),
                    ("format".to_owned(), "json".to_owned()),
                    ("action".to_owned(), "query".to_owned())
                    ]]);
    }

    #[tokio::test]
    async fn random() {
        let wikipedia = Wikipedia::<MockClient>::default();
        wikipedia.client.response.lock().unwrap().push("{\"query\":{\"random\":[{\"title\":\"hello\"}, {\"title\":\"world\"}]}}".to_owned());
        assert_eq!(
                wikipedia.random().await.unwrap(),
                Some("hello".to_owned())
                );
        assert_eq!(*wikipedia.client.url.lock().unwrap(),
                vec!["https://en.wikipedia.org/w/api.php".to_owned()]);
        assert_eq!(*wikipedia.client.arguments.lock().unwrap(),
                vec![vec![
                    ("list".to_owned(), "random".to_owned()),
                    ("rnnamespace".to_owned(), "0".to_owned()),
                    ("rnlimit".to_owned(), "1".to_owned()),
                    ("format".to_owned(), "json".to_owned()),
                    ("action".to_owned(), "query".to_owned())
                    ]]);
    }

    #[tokio::test]
    async fn page_content() {
        let wikipedia = Wikipedia::<MockClient>::default();
        wikipedia.client.response.lock().unwrap().push("{\"query\":{\"pages\":{\"a\":{\"extract\":\"hello\"}}}}".to_owned());
        let page = wikipedia.page_from_pageid("4138548".to_owned());
        let html = page.get_content().await.unwrap();
        assert_eq!(
                html,
                "hello".to_owned()
                );
        assert_eq!(*wikipedia.client.url.lock().unwrap(),
                vec!["https://en.wikipedia.org/w/api.php".to_owned()]);
        assert_eq!(*wikipedia.client.arguments.lock().unwrap(),
                vec![vec![
                    ("prop".to_owned(), "extracts|revisions".to_owned()),
                    ("explaintext".to_owned(), "".to_owned()),
                    ("rvprop".to_owned(), "ids".to_owned()),
                    ("redirects".to_owned(), "".to_owned()),
                    ("format".to_owned(), "json".to_owned()),
                    ("action".to_owned(), "query".to_owned()),
                    ("pageids".to_owned(), "4138548".to_owned()),
                    ]]);
    }

    #[tokio::test]
    async fn page_html_content() {
        let wikipedia = Wikipedia::<MockClient>::default();
        wikipedia.client.response.lock().unwrap().push("{\"query\":{\"pages\":{\"a\":{\"revisions\":[{\"*\":\"hello\"}]}}}}".to_owned());
        let page = wikipedia.page_from_pageid("4138548".to_owned());
        let html = page.get_html_content().await.unwrap();
        assert_eq!(
                html,
                "hello".to_owned()
                );
        assert_eq!(*wikipedia.client.url.lock().unwrap(),
                vec!["https://en.wikipedia.org/w/api.php".to_owned()]);
        assert_eq!(*wikipedia.client.arguments.lock().unwrap(),
                vec![vec![
                    ("prop".to_owned(), "revisions".to_owned()),
                    ("rvprop".to_owned(), "content".to_owned()),
                    ("rvlimit".to_owned(), "1".to_owned()),
                    ("rvparse".to_owned(), "".to_owned()),
                    ("redirects".to_owned(), "".to_owned()),
                    ("format".to_owned(), "json".to_owned()),
                    ("action".to_owned(), "query".to_owned()),
                    ("pageids".to_owned(), "4138548".to_owned()),
                    ]]);
    }

    #[tokio::test]
    async fn page_summary() {
        let wikipedia = Wikipedia::<MockClient>::default();
        wikipedia.client.response.lock().unwrap().push("{\"query\":{\"pages\":{\"a\":{\"extract\":\"hello\"}}}}".to_owned());
        let page = wikipedia.page_from_title("Parkinson's law of triviality".to_owned());
        let summary = page.get_summary().await.unwrap();
        assert_eq!(
                summary,
                "hello".to_owned()
                );
        assert_eq!(*wikipedia.client.url.lock().unwrap(),
                vec!["https://en.wikipedia.org/w/api.php".to_owned()]);
        assert_eq!(*wikipedia.client.arguments.lock().unwrap(),
                vec![vec![
                    ("prop".to_owned(), "extracts".to_owned()),
                    ("explaintext".to_owned(), "".to_owned()),
                    ("exintro".to_owned(), "".to_owned()),
                    ("redirects".to_owned(), "".to_owned()),
                    ("format".to_owned(), "json".to_owned()),
                    ("action".to_owned(), "query".to_owned()),
                    ("titles".to_owned(), "Parkinson\'s law of triviality".to_owned())
                    ]]);
    }

    #[tokio::test]
    async fn page_redirect_summary() {
        let wikipedia = Wikipedia::<MockClient>::default();
        wikipedia.client.response.lock().unwrap().push("{\"query\":{\"redirects\":[{\"to\":\"hello world\"}]}}".to_owned());
        wikipedia.client.response.lock().unwrap().push("{\"query\":{\"pages\":{\"a\":{\"extract\":\"hello\"}}}}".to_owned());
        let page = wikipedia.page_from_title("Parkinson's law of triviality".to_owned());
        let summary = page.get_summary().await.unwrap();
        assert_eq!(
                summary,
                "hello".to_owned()
                );
        assert_eq!(*wikipedia.client.url.lock().unwrap(),
                vec![
                "https://en.wikipedia.org/w/api.php".to_owned(),
                "https://en.wikipedia.org/w/api.php".to_owned(),
                ]);
        assert_eq!(*wikipedia.client.arguments.lock().unwrap(),
                vec![
                vec![
                    ("prop".to_owned(), "extracts".to_owned()),
                    ("explaintext".to_owned(), "".to_owned()),
                    ("exintro".to_owned(), "".to_owned()),
                    ("redirects".to_owned(), "".to_owned()),
                    ("format".to_owned(), "json".to_owned()),
                    ("action".to_owned(), "query".to_owned()),
                    ("titles".to_owned(), "Parkinson\'s law of triviality".to_owned())
                ],
                vec![
                    ("prop".to_owned(), "extracts".to_owned()),
                    ("explaintext".to_owned(), "".to_owned()),
                    ("exintro".to_owned(), "".to_owned()),
                    ("redirects".to_owned(), "".to_owned()),
                    ("format".to_owned(), "json".to_owned()),
                    ("action".to_owned(), "query".to_owned()),
                    ("titles".to_owned(), "hello world".to_owned())
                    ]
                ]
                );
    }

    #[tokio::test]
    async fn page_images() {
        let wikipedia = Wikipedia::<MockClient>::default();
        wikipedia.client.response.lock().unwrap().push("{\"continue\": {\"lol\":\"1\"},\"query\":{\"pages\":{\"a\":{\"title\":\"Image 1\", \"imageinfo\":[{\"url\": \"http://example.com/image1.jpg\", \"descriptionurl\": \"http://example.com/image1.jpg.html\"}]}}}}".to_owned());
        wikipedia.client.response.lock().unwrap().push("{\"query\":{\"pages\":{\"a\":{\"title\":\"Image 2\", \"imageinfo\":[{\"url\": \"http://example.com/image2.jpg\", \"descriptionurl\": \"http://example.com/image2.jpg.html\"}]}}}}".to_owned());
        let page = wikipedia.page_from_title("Parkinson's law of triviality".to_owned());
        let images= page.get_images().await.unwrap().collect_vec::<Vec<_>>().await;
        assert_eq!(
                images,
                vec![
                iter::Image {
                    url: "http://example.com/image1.jpg".to_owned(),
                    title: "Image 1".to_owned(),
                    description_url: "http://example.com/image1.jpg.html".to_owned(),
                },
                iter::Image {
                    url: "http://example.com/image2.jpg".to_owned(),
                    title: "Image 2".to_owned(),
                    description_url: "http://example.com/image2.jpg.html".to_owned(),
                }
                ]);
        assert_eq!(*wikipedia.client.url.lock().unwrap(),
                vec![
                "https://en.wikipedia.org/w/api.php".to_owned(),
                "https://en.wikipedia.org/w/api.php".to_owned(),
                ]);
        assert_eq!(*wikipedia.client.arguments.lock().unwrap(),
                vec![
                vec![
                    ("generator".to_owned(), "images".to_owned()),
                    ("gimlimit".to_owned(), "max".to_owned()),
                    ("prop".to_owned(), "imageinfo".to_owned()),
                    ("iiprop".to_owned(), "url".to_owned()),
                    ("format".to_owned(), "json".to_owned()),
                    ("action".to_owned(), "query".to_owned()),
                    ("titles".to_owned(), "Parkinson\'s law of triviality".to_owned()),
                    ("continue".to_owned(), "".to_owned())
                ],
                vec![
                    ("generator".to_owned(), "images".to_owned()),
                    ("gimlimit".to_owned(), "max".to_owned()),
                    ("prop".to_owned(), "imageinfo".to_owned()),
                    ("iiprop".to_owned(), "url".to_owned()),
                    ("format".to_owned(), "json".to_owned()),
                    ("action".to_owned(), "query".to_owned()),
                    ("titles".to_owned(), "Parkinson\'s law of triviality".to_owned()),
                    ("lol".to_owned(), "1".to_owned())
                ]
                ]
                );
    }

    #[tokio::test]
    async fn page_coordinates() {
        let wikipedia = Wikipedia::<MockClient>::default();
        wikipedia.client.response.lock().unwrap().push("{\"query\":{\"pages\":{\"a\":{\"coordinates\":[{\"lat\": 2.1, \"lon\":-1.3}]}}}}".to_owned());
        let page = wikipedia.page_from_title("World".to_owned());
        let coordinates = page.get_coordinates().await.unwrap().unwrap();
        assert_eq!(
                coordinates,
                (2.1, -1.3)
                );
        assert_eq!(*wikipedia.client.url.lock().unwrap(),
                vec!["https://en.wikipedia.org/w/api.php".to_owned()]);
        assert_eq!(*wikipedia.client.arguments.lock().unwrap(),
                vec![vec![
                    ("prop".to_owned(), "coordinates".to_owned()),
                    ("colimit".to_owned(), "max".to_owned()),
                    ("redirects".to_owned(), "".to_owned()),
                    ("format".to_owned(), "json".to_owned()),
                    ("action".to_owned(), "query".to_owned()),
                    ("titles".to_owned(), "World".to_owned())
                    ]]);
    }

    #[tokio::test]
    async fn page_no_coordinates() {
        let wikipedia = Wikipedia::<MockClient>::default();
        wikipedia.client.response.lock().unwrap().push("{\"query\":{\"pages\":{\"a\":{}}}}".to_owned());
        let page = wikipedia.page_from_title("World".to_owned());
        assert!(page.get_coordinates().await.unwrap().is_none());
        assert_eq!(*wikipedia.client.url.lock().unwrap(),
                vec!["https://en.wikipedia.org/w/api.php".to_owned()]);
        assert_eq!(*wikipedia.client.arguments.lock().unwrap(),
                vec![vec![
                    ("prop".to_owned(), "coordinates".to_owned()),
                    ("colimit".to_owned(), "max".to_owned()),
                    ("redirects".to_owned(), "".to_owned()),
                    ("format".to_owned(), "json".to_owned()),
                    ("action".to_owned(), "query".to_owned()),
                    ("titles".to_owned(), "World".to_owned())
                    ]]);
    }

    #[tokio::test]
    async fn get_references() {
        let wikipedia = Wikipedia::<MockClient>::default();
        wikipedia.client.response.lock().unwrap().push("{\"continue\": {\"lol\":\"1\"},\"query\":{\"pages\":{\"a\":{\"extlinks\":[{\"*\": \"//example.com/reference1.html\"}]}}}}".to_owned());
        wikipedia.client.response.lock().unwrap().push("{\"query\":{\"pages\":{\"a\":{\"extlinks\":[{\"*\": \"//example.com/reference2.html\"}]}}}}".to_owned());
        let page = wikipedia.page_from_title("World".to_owned());
        assert_eq!(
                page.get_references().await.unwrap().collect_vec::<Vec<_>>().await,
                vec![
                iter::Reference {
                    url: "http://example.com/reference1.html".to_owned(),
                },
                iter::Reference {
                    url: "http://example.com/reference2.html".to_owned(),
                }
                ]);
        assert_eq!(*wikipedia.client.url.lock().unwrap(),
                vec![
                "https://en.wikipedia.org/w/api.php".to_owned(),
                "https://en.wikipedia.org/w/api.php".to_owned(),
                ]);
        assert_eq!(*wikipedia.client.arguments.lock().unwrap(),
                vec![vec![
                    ("prop".to_owned(), "extlinks".to_owned()),
                    ("ellimit".to_owned(), "max".to_owned()),
                    ("format".to_owned(), "json".to_owned()),
                    ("action".to_owned(), "query".to_owned()),
                    ("titles".to_owned(), "World".to_owned()),
                    ("continue".to_owned(), "".to_owned())
                ],
                vec![
                    ("prop".to_owned(), "extlinks".to_owned()),
                    ("ellimit".to_owned(), "max".to_owned()),
                    ("format".to_owned(), "json".to_owned()),
                    ("action".to_owned(), "query".to_owned()),
                    ("titles".to_owned(), "World".to_owned()),
                    ("lol".to_owned(), "1".to_owned())
                ]
                ]);
    }

    #[tokio::test]
    async fn get_links() {
        let wikipedia = Wikipedia::<MockClient>::default();
        wikipedia.client.response.lock().unwrap().push("{\"continue\": {\"lol\":\"1\"},\"query\":{\"pages\":{\"a\":{\"links\":[{\"title\": \"Hello\"}]}}}}".to_owned());
        wikipedia.client.response.lock().unwrap().push("{\"query\":{\"pages\":{\"a\":{\"links\":[{\"title\": \"World\"}]}}}}".to_owned());
        let page = wikipedia.page_from_title("World".to_owned());
        assert_eq!(
                page.get_links().await.unwrap().collect_vec::<Vec<_>>().await,
                vec![
                iter::Link {
                    title: "Hello".to_owned(),
                },
                iter::Link {
                    title: "World".to_owned(),
                }
                ]);
        assert_eq!(*wikipedia.client.url.lock().unwrap(),
                vec![
                "https://en.wikipedia.org/w/api.php".to_owned(),
                "https://en.wikipedia.org/w/api.php".to_owned(),
                ]);
        assert_eq!(*wikipedia.client.arguments.lock().unwrap(),
                vec![
                vec![
                    ("prop".to_owned(), "links".to_owned()),
                    ("plnamespace".to_owned(), "0".to_owned()),
                    ("ellimit".to_owned(), "max".to_owned()),
                    ("format".to_owned(), "json".to_owned()),
                    ("action".to_owned(), "query".to_owned()),
                    ("titles".to_owned(), "World".to_owned()),
                    ("continue".to_owned(), "".to_owned()),
                ],
                vec![
                    ("prop".to_owned(), "links".to_owned()),
                    ("plnamespace".to_owned(), "0".to_owned()),
                    ("ellimit".to_owned(), "max".to_owned()),
                    ("format".to_owned(), "json".to_owned()),
                    ("action".to_owned(), "query".to_owned()),
                    ("titles".to_owned(), "World".to_owned()),
                    ("lol".to_owned(), "1".to_owned()),
                ]
                ]);
    }

    #[tokio::test]
    async fn get_categories() {
        let wikipedia = Wikipedia::<MockClient>::default();
        wikipedia.client.response.lock().unwrap().push("{\"continue\": {\"lol\":\"1\"},\"query\":{\"pages\":{\"a\":{\"categories\":[{\"title\": \"Hello\"}]}}}}".to_owned());
        wikipedia.client.response.lock().unwrap().push("{\"query\":{\"pages\":{\"a\":{\"categories\":[{\"title\": \"Category: World\"}]}}}}".to_owned());
        let page = wikipedia.page_from_title("World".to_owned());
        assert_eq!(
                page.get_categories().await.unwrap().collect_vec::<Vec<_>>().await,
                vec![
                iter::Category {
                    title: "Hello".to_owned(),
                },
                iter::Category {
                    title: "World".to_owned(),
                }
                ]);
        assert_eq!(*wikipedia.client.url.lock().unwrap(),
                vec![
                "https://en.wikipedia.org/w/api.php".to_owned(),
                "https://en.wikipedia.org/w/api.php".to_owned(),
                ]);
        assert_eq!(*wikipedia.client.arguments.lock().unwrap(),
                vec![
                vec![
                    ("prop".to_owned(), "categories".to_owned()),
                    ("cllimit".to_owned(), "max".to_owned()),
                    ("format".to_owned(), "json".to_owned()),
                    ("action".to_owned(), "query".to_owned()),
                    ("titles".to_owned(), "World".to_owned()),
                    ("continue".to_owned(), "".to_owned()),
                ],
                vec![
                    ("prop".to_owned(), "categories".to_owned()),
                    ("cllimit".to_owned(), "max".to_owned()),
                    ("format".to_owned(), "json".to_owned()),
                    ("action".to_owned(), "query".to_owned()),
                    ("titles".to_owned(), "World".to_owned()),
                    ("lol".to_owned(), "1".to_owned()),
                ]
                ]);
    }

    #[tokio::test]
    async fn sections() {
        let wikipedia = Wikipedia::<MockClient>::default();
        wikipedia.client.response.lock().unwrap().push("{\"parse\":{\"sections\":[{\"line\":\"hello\"}, {\"line\":\"world\"}]}}".to_owned());
        let page = wikipedia.page_from_pageid("123".to_owned());
        assert_eq!(
                page.get_sections().await.unwrap(),
                vec!["hello".to_owned(), "world".to_owned()]
                );
        assert_eq!(*wikipedia.client.url.lock().unwrap(),
                vec!["https://en.wikipedia.org/w/api.php".to_owned()]);
        assert_eq!(*wikipedia.client.arguments.lock().unwrap(),
                vec![vec![
                    ("prop".to_owned(), "sections".to_owned()),
                    ("format".to_owned(), "json".to_owned()),
                    ("action".to_owned(), "parse".to_owned()),
                    ("pageid".to_owned(), "123".to_owned())
                    ]]);
    }

    #[tokio::test]
    async fn languages() {
        let wikipedia = Wikipedia::<MockClient>::default();
        wikipedia.client.response.lock().unwrap().push("{\"query\":{\"languages\":[{\"*\":\"hello\", \"code\":\"world\"}, {\"*\":\"foo\", \"code\":\"bar\"}]}}".to_owned());
        assert_eq!(
            wikipedia.get_languages().await.unwrap(),
            vec![
                ("world".to_owned(), "hello".to_owned()),
                ("bar".to_owned(), "foo".to_owned()),
            ]
        );
        assert_eq!(*wikipedia.client.url.lock().unwrap(),
                   vec!["https://en.wikipedia.org/w/api.php".to_owned()]);
        assert_eq!(*wikipedia.client.arguments.lock().unwrap(),
                   vec![vec![
                       ("meta".to_owned(), "siteinfo".to_owned()),
                       ("siprop".to_owned(), "languages".to_owned()),
                       ("format".to_owned(), "json".to_owned()),
                       ("action".to_owned(), "query".to_owned())
                   ]]);
    }
}

#[cfg(test)]
mod wasm_tests {
    use super::Wikipedia;
    use super::http::HttpClient;
    use super::iter;
    use std::sync::Mutex;

    use crate::iter::AsyncIterator;

    use wasm_bindgen_test::wasm_bindgen_test;

    struct MockClient {
        pub url: Mutex<Vec<String>>,
        pub user_agent: Option<String>,
        pub arguments: Mutex<Vec<Vec<(String, String)>>>,
        pub response: Mutex<Vec<String>>,
    }

    impl Default for crate::wasm_tests::MockClient {
        fn default() -> Self {
            crate::wasm_tests::MockClient {
                url: Mutex::new(Vec::new()),
                user_agent: None,
                arguments: Mutex::new(Vec::new()),
                response: Mutex::new(Vec::new()),
            }
        }
    }

    impl super::http::HttpClient for crate::wasm_tests::MockClient {
        fn user_agent(&mut self, user_agent: String) {
            self.user_agent = Some(user_agent)
        }

        async fn get<'a, I>(&self, base_url: &str, args: I) -> Result<String, super::http::Error>
        where I: Iterator<Item=(&'a str, &'a str)> {
            self.url.lock().unwrap().push(base_url.to_owned());
            self.arguments.lock().unwrap().push(args.map(|x| (x.0.to_owned(), x.1.to_owned())).collect());
            Ok(self.response.lock().unwrap().remove(0))
        }
    }

    #[wasm_bindgen_test]
    fn base_url() {
        let mut wikipedia = Wikipedia::<MockClient>::default();
        assert_eq!(wikipedia.base_url(), "https://en.wikipedia.org/w/api.php");
        wikipedia.language = "es".to_owned();
        assert_eq!(wikipedia.base_url(), "https://es.wikipedia.org/w/api.php");

        wikipedia.set_base_url("https://hello.{language}.world/");
        assert_eq!(wikipedia.base_url(), "https://hello.es.world/");

        wikipedia.set_base_url("https://hello.world/");
        assert_eq!(wikipedia.base_url(), "https://hello.world/");
    }

    #[wasm_bindgen_test]
    async fn user_agent() {
        let mut wikipedia = Wikipedia::<MockClient>::default();
        wikipedia.client.response.lock().unwrap().push("{}".to_owned());
        wikipedia.search("hello world").await.unwrap_err();
        assert_eq!(&*wikipedia.client.user_agent.unwrap(), "wikipedia (https://github.com/seppo0010/wikipedia-rs)");

        let mut client = MockClient::default();
        client.user_agent("hello world".to_owned());
        client.response.lock().unwrap().push("{}".to_owned());
        wikipedia.client = client;
        wikipedia.search("hello world").await.unwrap_err();
        assert_eq!(&*wikipedia.client.user_agent.unwrap(), "hello world");
    }

    #[wasm_bindgen_test]
    async fn search() {
        let wikipedia = Wikipedia::<MockClient>::default();
        wikipedia.client.response.lock().unwrap().push("{\"query\":{\"search\":[{\"title\":\"hello\"}, {\"title\":\"world\"}]}}".to_owned());
        assert_eq!(
            wikipedia.search("hello world").await.unwrap(),
            vec![
                "hello".to_owned(),
                "world".to_owned(),
            ]);
        assert_eq!(*wikipedia.client.url.lock().unwrap(),
                   vec!["https://en.wikipedia.org/w/api.php".to_owned()]);
        assert_eq!(*wikipedia.client.arguments.lock().unwrap(),
                   vec![vec![
                       ("list".to_owned(), "search".to_owned()),
                       ("srprop".to_owned(), "".to_owned()),
                       ("srlimit".to_owned(), "10".to_owned()),
                       ("srsearch".to_owned(), "hello world".to_owned()),
                       ("format".to_owned(), "json".to_owned()),
                       ("action".to_owned(), "query".to_owned())
                   ]]);
    }

    #[wasm_bindgen_test]
    async fn geosearch() {
        let wikipedia = Wikipedia::<MockClient>::default();
        wikipedia.client.response.lock().unwrap().push("{\"query\":{\"geosearch\":[{\"title\":\"hello\"}, {\"title\":\"world\"}]}}".to_owned());
        assert_eq!(
            wikipedia.geosearch(-34.603333, -58.381667, 10).await.unwrap(),
            vec![
                "hello".to_owned(),
                "world".to_owned(),
            ]);
        assert_eq!(*wikipedia.client.url.lock().unwrap(),
                   vec!["https://en.wikipedia.org/w/api.php".to_owned()]);
        assert_eq!(*wikipedia.client.arguments.lock().unwrap(),
                   vec![vec![
                       ("list".to_owned(), "geosearch".to_owned()),
                       ("gsradius".to_owned(), "10".to_owned()),
                       ("gscoord".to_owned(), "-34.603333|-58.381667".to_owned()),
                       ("gslimit".to_owned(), "10".to_owned()),
                       ("format".to_owned(), "json".to_owned()),
                       ("action".to_owned(), "query".to_owned())
                   ]]);
    }

    #[wasm_bindgen_test]
    async fn random_count() {
        let wikipedia = Wikipedia::<MockClient>::default();
        wikipedia.client.response.lock().unwrap().push("{\"query\":{\"random\":[{\"title\":\"hello\"}, {\"title\":\"world\"}]}}".to_owned());
        assert_eq!(
            wikipedia.random_count(10).await.unwrap(),
            vec![
                "hello".to_owned(),
                "world".to_owned(),
            ]);
        assert_eq!(*wikipedia.client.url.lock().unwrap(),
                   vec!["https://en.wikipedia.org/w/api.php".to_owned()]);
        assert_eq!(*wikipedia.client.arguments.lock().unwrap(),
                   vec![vec![
                       ("list".to_owned(), "random".to_owned()),
                       ("rnnamespace".to_owned(), "0".to_owned()),
                       ("rnlimit".to_owned(), "10".to_owned()),
                       ("format".to_owned(), "json".to_owned()),
                       ("action".to_owned(), "query".to_owned())
                   ]]);
    }

    #[wasm_bindgen_test]
    async fn random() {
        let wikipedia = Wikipedia::<MockClient>::default();
        wikipedia.client.response.lock().unwrap().push("{\"query\":{\"random\":[{\"title\":\"hello\"}, {\"title\":\"world\"}]}}".to_owned());
        assert_eq!(
            wikipedia.random().await.unwrap(),
            Some("hello".to_owned())
        );
        assert_eq!(*wikipedia.client.url.lock().unwrap(),
                   vec!["https://en.wikipedia.org/w/api.php".to_owned()]);
        assert_eq!(*wikipedia.client.arguments.lock().unwrap(),
                   vec![vec![
                       ("list".to_owned(), "random".to_owned()),
                       ("rnnamespace".to_owned(), "0".to_owned()),
                       ("rnlimit".to_owned(), "1".to_owned()),
                       ("format".to_owned(), "json".to_owned()),
                       ("action".to_owned(), "query".to_owned())
                   ]]);
    }

    #[wasm_bindgen_test]
    async fn page_content() {
        let wikipedia = Wikipedia::<MockClient>::default();
        wikipedia.client.response.lock().unwrap().push("{\"query\":{\"pages\":{\"a\":{\"extract\":\"hello\"}}}}".to_owned());
        let page = wikipedia.page_from_pageid("4138548".to_owned());
        let html = page.get_content().await.unwrap();
        assert_eq!(
            html,
            "hello".to_owned()
        );
        assert_eq!(*wikipedia.client.url.lock().unwrap(),
                   vec!["https://en.wikipedia.org/w/api.php".to_owned()]);
        assert_eq!(*wikipedia.client.arguments.lock().unwrap(),
                   vec![vec![
                       ("prop".to_owned(), "extracts|revisions".to_owned()),
                       ("explaintext".to_owned(), "".to_owned()),
                       ("rvprop".to_owned(), "ids".to_owned()),
                       ("redirects".to_owned(), "".to_owned()),
                       ("format".to_owned(), "json".to_owned()),
                       ("action".to_owned(), "query".to_owned()),
                       ("pageids".to_owned(), "4138548".to_owned()),
                   ]]);
    }

    #[wasm_bindgen_test]
    async fn page_html_content() {
        let wikipedia = Wikipedia::<MockClient>::default();
        wikipedia.client.response.lock().unwrap().push("{\"query\":{\"pages\":{\"a\":{\"revisions\":[{\"*\":\"hello\"}]}}}}".to_owned());
        let page = wikipedia.page_from_pageid("4138548".to_owned());
        let html = page.get_html_content().await.unwrap();
        assert_eq!(
            html,
            "hello".to_owned()
        );
        assert_eq!(*wikipedia.client.url.lock().unwrap(),
                   vec!["https://en.wikipedia.org/w/api.php".to_owned()]);
        assert_eq!(*wikipedia.client.arguments.lock().unwrap(),
                   vec![vec![
                       ("prop".to_owned(), "revisions".to_owned()),
                       ("rvprop".to_owned(), "content".to_owned()),
                       ("rvlimit".to_owned(), "1".to_owned()),
                       ("rvparse".to_owned(), "".to_owned()),
                       ("redirects".to_owned(), "".to_owned()),
                       ("format".to_owned(), "json".to_owned()),
                       ("action".to_owned(), "query".to_owned()),
                       ("pageids".to_owned(), "4138548".to_owned()),
                   ]]);
    }

    #[wasm_bindgen_test]
    async fn page_summary() {
        let wikipedia = Wikipedia::<MockClient>::default();
        wikipedia.client.response.lock().unwrap().push("{\"query\":{\"pages\":{\"a\":{\"extract\":\"hello\"}}}}".to_owned());
        let page = wikipedia.page_from_title("Parkinson's law of triviality".to_owned());
        let summary = page.get_summary().await.unwrap();
        assert_eq!(
            summary,
            "hello".to_owned()
        );
        assert_eq!(*wikipedia.client.url.lock().unwrap(),
                   vec!["https://en.wikipedia.org/w/api.php".to_owned()]);
        assert_eq!(*wikipedia.client.arguments.lock().unwrap(),
                   vec![vec![
                       ("prop".to_owned(), "extracts".to_owned()),
                       ("explaintext".to_owned(), "".to_owned()),
                       ("exintro".to_owned(), "".to_owned()),
                       ("redirects".to_owned(), "".to_owned()),
                       ("format".to_owned(), "json".to_owned()),
                       ("action".to_owned(), "query".to_owned()),
                       ("titles".to_owned(), "Parkinson\'s law of triviality".to_owned())
                   ]]);
    }

    #[wasm_bindgen_test]
    async fn page_redirect_summary() {
        let wikipedia = Wikipedia::<MockClient>::default();
        wikipedia.client.response.lock().unwrap().push("{\"query\":{\"redirects\":[{\"to\":\"hello world\"}]}}".to_owned());
        wikipedia.client.response.lock().unwrap().push("{\"query\":{\"pages\":{\"a\":{\"extract\":\"hello\"}}}}".to_owned());
        let page = wikipedia.page_from_title("Parkinson's law of triviality".to_owned());
        let summary = page.get_summary().await.unwrap();
        assert_eq!(
            summary,
            "hello".to_owned()
        );
        assert_eq!(*wikipedia.client.url.lock().unwrap(),
                   vec![
                       "https://en.wikipedia.org/w/api.php".to_owned(),
                       "https://en.wikipedia.org/w/api.php".to_owned(),
                   ]);
        assert_eq!(*wikipedia.client.arguments.lock().unwrap(),
                   vec![
                       vec![
                           ("prop".to_owned(), "extracts".to_owned()),
                           ("explaintext".to_owned(), "".to_owned()),
                           ("exintro".to_owned(), "".to_owned()),
                           ("redirects".to_owned(), "".to_owned()),
                           ("format".to_owned(), "json".to_owned()),
                           ("action".to_owned(), "query".to_owned()),
                           ("titles".to_owned(), "Parkinson\'s law of triviality".to_owned())
                       ],
                       vec![
                           ("prop".to_owned(), "extracts".to_owned()),
                           ("explaintext".to_owned(), "".to_owned()),
                           ("exintro".to_owned(), "".to_owned()),
                           ("redirects".to_owned(), "".to_owned()),
                           ("format".to_owned(), "json".to_owned()),
                           ("action".to_owned(), "query".to_owned()),
                           ("titles".to_owned(), "hello world".to_owned())
                       ]
                   ]
        );
    }

    #[wasm_bindgen_test]
    async fn page_images() {
        let wikipedia = Wikipedia::<MockClient>::default();
        wikipedia.client.response.lock().unwrap().push("{\"continue\": {\"lol\":\"1\"},\"query\":{\"pages\":{\"a\":{\"title\":\"Image 1\", \"imageinfo\":[{\"url\": \"http://example.com/image1.jpg\", \"descriptionurl\": \"http://example.com/image1.jpg.html\"}]}}}}".to_owned());
        wikipedia.client.response.lock().unwrap().push("{\"query\":{\"pages\":{\"a\":{\"title\":\"Image 2\", \"imageinfo\":[{\"url\": \"http://example.com/image2.jpg\", \"descriptionurl\": \"http://example.com/image2.jpg.html\"}]}}}}".to_owned());
        let page = wikipedia.page_from_title("Parkinson's law of triviality".to_owned());
        let images= page.get_images().await.unwrap().collect_vec::<Vec<_>>().await;
        assert_eq!(
            images,
            vec![
                iter::Image {
                    url: "http://example.com/image1.jpg".to_owned(),
                    title: "Image 1".to_owned(),
                    description_url: "http://example.com/image1.jpg.html".to_owned(),
                },
                iter::Image {
                    url: "http://example.com/image2.jpg".to_owned(),
                    title: "Image 2".to_owned(),
                    description_url: "http://example.com/image2.jpg.html".to_owned(),
                }
            ]);
        assert_eq!(*wikipedia.client.url.lock().unwrap(),
                   vec![
                       "https://en.wikipedia.org/w/api.php".to_owned(),
                       "https://en.wikipedia.org/w/api.php".to_owned(),
                   ]);
        assert_eq!(*wikipedia.client.arguments.lock().unwrap(),
                   vec![
                       vec![
                           ("generator".to_owned(), "images".to_owned()),
                           ("gimlimit".to_owned(), "max".to_owned()),
                           ("prop".to_owned(), "imageinfo".to_owned()),
                           ("iiprop".to_owned(), "url".to_owned()),
                           ("format".to_owned(), "json".to_owned()),
                           ("action".to_owned(), "query".to_owned()),
                           ("titles".to_owned(), "Parkinson\'s law of triviality".to_owned()),
                           ("continue".to_owned(), "".to_owned())
                       ],
                       vec![
                           ("generator".to_owned(), "images".to_owned()),
                           ("gimlimit".to_owned(), "max".to_owned()),
                           ("prop".to_owned(), "imageinfo".to_owned()),
                           ("iiprop".to_owned(), "url".to_owned()),
                           ("format".to_owned(), "json".to_owned()),
                           ("action".to_owned(), "query".to_owned()),
                           ("titles".to_owned(), "Parkinson\'s law of triviality".to_owned()),
                           ("lol".to_owned(), "1".to_owned())
                       ]
                   ]
        );
    }

    #[wasm_bindgen_test]
    async fn page_coordinates() {
        let wikipedia = Wikipedia::<MockClient>::default();
        wikipedia.client.response.lock().unwrap().push("{\"query\":{\"pages\":{\"a\":{\"coordinates\":[{\"lat\": 2.1, \"lon\":-1.3}]}}}}".to_owned());
        let page = wikipedia.page_from_title("World".to_owned());
        let coordinates = page.get_coordinates().await.unwrap().unwrap();
        assert_eq!(
            coordinates,
            (2.1, -1.3)
        );
        assert_eq!(*wikipedia.client.url.lock().unwrap(),
                   vec!["https://en.wikipedia.org/w/api.php".to_owned()]);
        assert_eq!(*wikipedia.client.arguments.lock().unwrap(),
                   vec![vec![
                       ("prop".to_owned(), "coordinates".to_owned()),
                       ("colimit".to_owned(), "max".to_owned()),
                       ("redirects".to_owned(), "".to_owned()),
                       ("format".to_owned(), "json".to_owned()),
                       ("action".to_owned(), "query".to_owned()),
                       ("titles".to_owned(), "World".to_owned())
                   ]]);
    }

    #[wasm_bindgen_test]
    async fn page_no_coordinates() {
        let wikipedia = Wikipedia::<MockClient>::default();
        wikipedia.client.response.lock().unwrap().push("{\"query\":{\"pages\":{\"a\":{}}}}".to_owned());
        let page = wikipedia.page_from_title("World".to_owned());
        assert!(page.get_coordinates().await.unwrap().is_none());
        assert_eq!(*wikipedia.client.url.lock().unwrap(),
                   vec!["https://en.wikipedia.org/w/api.php".to_owned()]);
        assert_eq!(*wikipedia.client.arguments.lock().unwrap(),
                   vec![vec![
                       ("prop".to_owned(), "coordinates".to_owned()),
                       ("colimit".to_owned(), "max".to_owned()),
                       ("redirects".to_owned(), "".to_owned()),
                       ("format".to_owned(), "json".to_owned()),
                       ("action".to_owned(), "query".to_owned()),
                       ("titles".to_owned(), "World".to_owned())
                   ]]);
    }

    #[wasm_bindgen_test]
    async fn get_references() {
        let wikipedia = Wikipedia::<MockClient>::default();
        wikipedia.client.response.lock().unwrap().push("{\"continue\": {\"lol\":\"1\"},\"query\":{\"pages\":{\"a\":{\"extlinks\":[{\"*\": \"//example.com/reference1.html\"}]}}}}".to_owned());
        wikipedia.client.response.lock().unwrap().push("{\"query\":{\"pages\":{\"a\":{\"extlinks\":[{\"*\": \"//example.com/reference2.html\"}]}}}}".to_owned());
        let page = wikipedia.page_from_title("World".to_owned());
        assert_eq!(
            page.get_references().await.unwrap().collect_vec::<Vec<_>>().await,
            vec![
                iter::Reference {
                    url: "http://example.com/reference1.html".to_owned(),
                },
                iter::Reference {
                    url: "http://example.com/reference2.html".to_owned(),
                }
            ]);
        assert_eq!(*wikipedia.client.url.lock().unwrap(),
                   vec![
                       "https://en.wikipedia.org/w/api.php".to_owned(),
                       "https://en.wikipedia.org/w/api.php".to_owned(),
                   ]);
        assert_eq!(*wikipedia.client.arguments.lock().unwrap(),
                   vec![vec![
                       ("prop".to_owned(), "extlinks".to_owned()),
                       ("ellimit".to_owned(), "max".to_owned()),
                       ("format".to_owned(), "json".to_owned()),
                       ("action".to_owned(), "query".to_owned()),
                       ("titles".to_owned(), "World".to_owned()),
                       ("continue".to_owned(), "".to_owned())
                   ],
                        vec![
                            ("prop".to_owned(), "extlinks".to_owned()),
                            ("ellimit".to_owned(), "max".to_owned()),
                            ("format".to_owned(), "json".to_owned()),
                            ("action".to_owned(), "query".to_owned()),
                            ("titles".to_owned(), "World".to_owned()),
                            ("lol".to_owned(), "1".to_owned())
                        ]
                   ]);
    }

    #[wasm_bindgen_test]
    async fn get_links() {
        let wikipedia = Wikipedia::<MockClient>::default();
        wikipedia.client.response.lock().unwrap().push("{\"continue\": {\"lol\":\"1\"},\"query\":{\"pages\":{\"a\":{\"links\":[{\"title\": \"Hello\"}]}}}}".to_owned());
        wikipedia.client.response.lock().unwrap().push("{\"query\":{\"pages\":{\"a\":{\"links\":[{\"title\": \"World\"}]}}}}".to_owned());
        let page = wikipedia.page_from_title("World".to_owned());
        assert_eq!(
            page.get_links().await.unwrap().collect_vec::<Vec<_>>().await,
            vec![
                iter::Link {
                    title: "Hello".to_owned(),
                },
                iter::Link {
                    title: "World".to_owned(),
                }
            ]);
        assert_eq!(*wikipedia.client.url.lock().unwrap(),
                   vec![
                       "https://en.wikipedia.org/w/api.php".to_owned(),
                       "https://en.wikipedia.org/w/api.php".to_owned(),
                   ]);
        assert_eq!(*wikipedia.client.arguments.lock().unwrap(),
                   vec![
                       vec![
                           ("prop".to_owned(), "links".to_owned()),
                           ("plnamespace".to_owned(), "0".to_owned()),
                           ("ellimit".to_owned(), "max".to_owned()),
                           ("format".to_owned(), "json".to_owned()),
                           ("action".to_owned(), "query".to_owned()),
                           ("titles".to_owned(), "World".to_owned()),
                           ("continue".to_owned(), "".to_owned()),
                       ],
                       vec![
                           ("prop".to_owned(), "links".to_owned()),
                           ("plnamespace".to_owned(), "0".to_owned()),
                           ("ellimit".to_owned(), "max".to_owned()),
                           ("format".to_owned(), "json".to_owned()),
                           ("action".to_owned(), "query".to_owned()),
                           ("titles".to_owned(), "World".to_owned()),
                           ("lol".to_owned(), "1".to_owned()),
                       ]
                   ]);
    }

    #[wasm_bindgen_test]
    async fn get_categories() {
        let wikipedia = Wikipedia::<MockClient>::default();
        wikipedia.client.response.lock().unwrap().push("{\"continue\": {\"lol\":\"1\"},\"query\":{\"pages\":{\"a\":{\"categories\":[{\"title\": \"Hello\"}]}}}}".to_owned());
        wikipedia.client.response.lock().unwrap().push("{\"query\":{\"pages\":{\"a\":{\"categories\":[{\"title\": \"Category: World\"}]}}}}".to_owned());
        let page = wikipedia.page_from_title("World".to_owned());
        assert_eq!(
            page.get_categories().await.unwrap().collect_vec::<Vec<_>>().await,
            vec![
                iter::Category {
                    title: "Hello".to_owned(),
                },
                iter::Category {
                    title: "World".to_owned(),
                }
            ]);
        assert_eq!(*wikipedia.client.url.lock().unwrap(),
                   vec![
                       "https://en.wikipedia.org/w/api.php".to_owned(),
                       "https://en.wikipedia.org/w/api.php".to_owned(),
                   ]);
        assert_eq!(*wikipedia.client.arguments.lock().unwrap(),
                   vec![
                       vec![
                           ("prop".to_owned(), "categories".to_owned()),
                           ("cllimit".to_owned(), "max".to_owned()),
                           ("format".to_owned(), "json".to_owned()),
                           ("action".to_owned(), "query".to_owned()),
                           ("titles".to_owned(), "World".to_owned()),
                           ("continue".to_owned(), "".to_owned()),
                       ],
                       vec![
                           ("prop".to_owned(), "categories".to_owned()),
                           ("cllimit".to_owned(), "max".to_owned()),
                           ("format".to_owned(), "json".to_owned()),
                           ("action".to_owned(), "query".to_owned()),
                           ("titles".to_owned(), "World".to_owned()),
                           ("lol".to_owned(), "1".to_owned()),
                       ]
                   ]);
    }

    #[wasm_bindgen_test]
    async fn sections() {
        let wikipedia = Wikipedia::<MockClient>::default();
        wikipedia.client.response.lock().unwrap().push("{\"parse\":{\"sections\":[{\"line\":\"hello\"}, {\"line\":\"world\"}]}}".to_owned());
        let page = wikipedia.page_from_pageid("123".to_owned());
        assert_eq!(
            page.get_sections().await.unwrap(),
            vec!["hello".to_owned(), "world".to_owned()]
        );
        assert_eq!(*wikipedia.client.url.lock().unwrap(),
                   vec!["https://en.wikipedia.org/w/api.php".to_owned()]);
        assert_eq!(*wikipedia.client.arguments.lock().unwrap(),
                   vec![vec![
                       ("prop".to_owned(), "sections".to_owned()),
                       ("format".to_owned(), "json".to_owned()),
                       ("action".to_owned(), "parse".to_owned()),
                       ("pageid".to_owned(), "123".to_owned())
                   ]]);
    }

    #[wasm_bindgen_test]
    async fn languages() {
        let wikipedia = Wikipedia::<MockClient>::default();
        wikipedia.client.response.lock().unwrap().push("{\"query\":{\"languages\":[{\"*\":\"hello\", \"code\":\"world\"}, {\"*\":\"foo\", \"code\":\"bar\"}]}}".to_owned());
        assert_eq!(
            wikipedia.get_languages().await.unwrap(),
            vec![
                ("world".to_owned(), "hello".to_owned()),
                ("bar".to_owned(), "foo".to_owned()),
            ]
        );
        assert_eq!(*wikipedia.client.url.lock().unwrap(),
                   vec!["https://en.wikipedia.org/w/api.php".to_owned()]);
        assert_eq!(*wikipedia.client.arguments.lock().unwrap(),
                   vec![vec![
                       ("meta".to_owned(), "siteinfo".to_owned()),
                       ("siprop".to_owned(), "languages".to_owned()),
                       ("format".to_owned(), "json".to_owned()),
                       ("action".to_owned(), "query".to_owned())
                   ]]);
    }
}
