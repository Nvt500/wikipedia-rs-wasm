#![allow(async_fn_in_trait)]
use std::vec::IntoIter;
use std::marker::PhantomData;

use serde_json::Value;

use super::{Page, Result, http};


pub trait AsyncIterator
{
    type Item;

    async fn next(&mut self) -> Option<Self::Item>;

    /// Collect to a vector
    async fn collect_vec<B: FromAsyncIterator<Self::Item>>(self) -> B
    where
        Self: Sized,
    {
        let fut = <B as FromAsyncIterator<_>>::from_iter(self);
        fut.await
    }

    fn size_hint(&self) -> (usize, Option<usize>)
    {
        (0, None)
    }

    /// Basic for each
    async fn for_each<T>(&mut self, mut func: impl FnMut(Self::Item))
    {
        while let Some(item) = self.next().await
        {
            func(item);
        }
    }

    /// Basic for each but when a None is returned it stops
    /// ```rust
    /// use wikipedia_wasm::{Wikipedia, http};
    /// use wikipedia_wasm::iter::AsyncIterator;
    ///
    /// #[tokio::main]
    /// async fn main()
    /// {
    ///     let wiki = Wikipedia::<http::default::Client>::default();
    ///     let page = wiki.page_from_title("World War II".to_string());
    ///     let mut images = page.get_images().await.unwrap();
    ///     let mut limited_images = Vec::new();
    ///     images.for_each_interrupted(|i| {
    ///         limited_images.push(i);
    ///         if limited_images.len() == 4 {
    ///             return None;
    ///         }
    ///         Some(())
    ///     }).await;
    ///     assert_eq!(limited_images.len(), 4);
    /// }
    /// ```
    async fn for_each_interrupted(&mut self, mut func: impl FnMut(Self::Item) -> Option<()>)
    {
        while let Some(item) = self.next().await
        {
            if func(item).is_none()
            {
                break;
            }
        }
    }
}

pub trait FromAsyncIterator<A>: Sized
{
    async fn from_iter<T: IntoAsyncIterator<Item = A>>(iter: T) -> Self;
}

impl<T> FromAsyncIterator<T> for Vec<T>
{
    async fn from_iter<I: IntoAsyncIterator<Item = T>>(iter: I) -> Vec<T>
    {
        let mut iter = iter.into_iter();
        let mut output = Vec::with_capacity(iter.size_hint().1.unwrap_or_default());
        while let Some(item) = iter.next().await
        {
            output.push(item);
        }
        output
    }
}

pub trait IntoAsyncIterator
{
    type Item;
    type IntoIter: AsyncIterator<Item = Self::Item>;

    fn into_iter(self) -> Self::IntoIter;
}

impl<I: AsyncIterator> IntoAsyncIterator for I
{
    type Item = I::Item;
    type IntoIter = I;

    fn into_iter(self) -> I
    {
        self
    }
}

pub struct Iter<'a, A: 'a + http::HttpClient, B: IterItem> {
    page: &'a Page<'a, A>,
    inner: IntoIter<Value>,
    cont: Option<Vec<(String, String)>>,
    phantom: PhantomData<B>
}

impl<'a, A: http::HttpClient, B: IterItem> Iter<'a, A, B> {
    pub async fn new(page: &'a Page<'_, A>) -> Result<Iter<'a, A, B>> {
        let (array, cont) = B::request_next(page, &None).await?;
        Ok(Iter {
            page: page,
            inner: array.into_iter(),
            cont: cont,
            phantom: PhantomData,
        })
    }

    async fn fetch_next(&mut self) -> Result <()> {
        if self.cont.is_some() {
            let (array, cont) = B::request_next(self.page, &self.cont).await?;
            self.inner = array.into_iter();
            self.cont = cont;
        }
        Ok(())
    }
}

impl<'a, A: http::HttpClient, B: IterItem> AsyncIterator for Iter<'a, A, B> {
    type Item = B;
    async fn next(&mut self) -> Option<Self::Item> {
        match self.inner.next() {
            Some(ref v) => B::from_value(&v),
            None => match self.cont {
                Some(_) => match self.fetch_next().await {
                    Ok(_) => self.inner.next().and_then(|x| B::from_value(&x)),
                    Err(_) => None,
                },
                None => None,
            }
        }
    }
}

pub trait IterItem: Sized {
    async fn request_next<A: http::HttpClient>(page: &Page<A>, cont: &Option<Vec<(String, String)>>)
            -> Result<(Vec<Value>, Option<Vec<(String, String)>>)>;
    fn from_value(value: &Value) -> Option<Self>;
}

#[derive(Debug, PartialEq)]
pub struct Image {
    pub url: String,
    pub title: String,
    pub description_url: String,
}

impl IterItem for Image {
    async fn request_next<A: http::HttpClient>(page: &Page<'_, A>, cont: &Option<Vec<(String, String)>>)
                                               -> Result<(Vec<Value>, Option<Vec<(String, String)>>)> {
        page.request_images(&cont).await
    }

    fn from_value(value: &Value) -> Option<Image> {
        let obj = match value.as_object() {
            Some(o) => o,
            None => return None,
        };

        let title = obj
            .get("title")
            .and_then(|x| x.as_str())
            .unwrap_or("").to_owned();
        let url = obj
            .get("imageinfo")
            .and_then(|x| x.as_array())
            .and_then(|x| x.into_iter().next())
            .and_then(|x| x.as_object())
            .and_then(|x| x.get("url"))
            .and_then(|x| x.as_str())
            .unwrap_or("").to_owned();
        let description_url = obj
            .get("imageinfo")
            .and_then(|x| x.as_array())
            .and_then(|x| x.into_iter().next())
            .and_then(|x| x.as_object())
            .and_then(|x| x.get("descriptionurl"))
            .and_then(|x| x.as_str())
            .unwrap_or("").to_owned();

        Some(Image {
            url: url.to_owned(),
            title: title.to_owned(),
            description_url: description_url.to_owned(),
        })
    }
}

#[derive(Debug, PartialEq)]
pub struct Reference {
    pub url: String,
}

impl IterItem for Reference {
    async fn request_next<A: http::HttpClient>(page: &Page<'_, A>, cont: &Option<Vec<(String, String)>>)
                                               -> Result<(Vec<Value>, Option<Vec<(String, String)>>)> {
        page.request_extlinks(&cont).await
    }

    fn from_value(value: &Value) -> Option<Reference> {
        value
            .as_object()
            .and_then(|x| x.get("*"))
            .and_then(|x| x.as_str())
            .map(|s| Reference {
                url: if s.starts_with("http:") {
                    s.to_owned()
                } else {
                    format!("http:{}", s)
                },
            })
    }
}

#[derive(Debug, PartialEq)]
pub struct Link {
    pub title: String,
}

impl IterItem for Link {
    async fn request_next<A: http::HttpClient>(page: &Page<'_, A>, cont: &Option<Vec<(String, String)>>)
                                               -> Result<(Vec<Value>, Option<Vec<(String, String)>>)> {
        page.request_links(&cont).await
    }

    fn from_value(value: &Value) -> Option<Link> {
        value
            .as_object()
            .and_then(|x| x.get("title"))
            .and_then(|x| x.as_str())
            .map(|s| Link { title: s.to_owned() })
    }
}

#[derive(Debug, PartialEq)]
pub struct LangLink {
    /// The language ID
    pub lang: String,

    /// The page title in this language, may be `None` if undefined
    pub title: Option<String>,
}

impl IterItem for LangLink {
    async fn request_next<A: http::HttpClient>(page: &Page<'_, A>, cont: &Option<Vec<(String, String)>>)
                                               -> Result<(Vec<Value>, Option<Vec<(String, String)>>)> {
        page.request_langlinks(&cont).await
    }

    fn from_value(value: &Value) -> Option<LangLink> {
        value
            .as_object()
            .map(|l| LangLink {
                lang: l.get("lang").unwrap().as_str().unwrap().into(),
                title: l.get("*").and_then(|n| n.as_str()).map(|n| n.into()),
            })
    }
}

#[derive(Debug, PartialEq)]
pub struct Category {
    pub title: String,
}

impl IterItem for Category {
    async fn request_next<A: http::HttpClient>(page: &Page<'_, A>, cont: &Option<Vec<(String, String)>>)
                                               -> Result<(Vec<Value>, Option<Vec<(String, String)>>)> {
        page.request_categories(&cont).await
    }

    fn from_value(value: &Value) -> Option<Category> {
        value
            .as_object()
            .and_then(|x| x.get("title"))
            .and_then(|x| x.as_str())
            .map(|s| Category {
                title: if s.starts_with("Category: ") {
                    s[10..].to_owned()
                } else {
                    s.to_owned()
                },
            })
    }
}
