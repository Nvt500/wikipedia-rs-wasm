extern crate wikipedia_wasm;

#[cfg(feature = "http-client")]
mod tests {
    use wikipedia_wasm::Wikipedia;
    use wikipedia_wasm::http;
    use std::collections::HashSet;

    use crate::wikipedia_wasm::iter::AsyncIterator;

    fn w() -> Wikipedia<http::default::Client> {
        Wikipedia::default()
    }

    #[tokio::test]
    async fn search() {
        let wikipedia = w();
        let results = wikipedia.search("hello world").await.unwrap();
        assert!(results.len() > 0);
        assert!(results.contains(&"\"Hello, World!\" program".to_owned()));
    }

    #[tokio::test]
    async fn geosearch() {
        let wikipedia = w();
        let results = wikipedia.geosearch(-34.603333, -58.381667, 10).await.unwrap();
        assert!(results.len() > 0);
        assert!(results.contains(&"Buenos Aires".to_owned()));
    }

    #[tokio::test]
    async fn random() {
        let wikipedia = w();
        wikipedia.random().await.unwrap().unwrap();
    }

    #[tokio::test]
    async fn random_count() {
        let wikipedia = w();
        assert_eq!(wikipedia.random_count(3).await.unwrap().len(), 3);
    }

    #[tokio::test]
    async fn page_content() {
        let wikipedia = w();
        let page = wikipedia.page_from_title("Parkinson's law of triviality".to_owned());
        assert!(page.get_content().await.unwrap().contains("bike-shedding"));
    }

    #[tokio::test]
    async fn title() {
        let wikipedia = w();
        let page = wikipedia.page_from_title("Parkinson's law of triviality".to_owned());
        assert_eq!(page.get_title().await.unwrap(), "Parkinson's law of triviality".to_owned());
        let page = wikipedia.page_from_pageid("4138548".to_owned());
        assert_eq!(page.get_title().await.unwrap(), "Law of triviality".to_owned());
    }

    #[tokio::test]
    async fn pageid() {
        let wikipedia = w();
        let page = wikipedia.page_from_title("Parkinson's law of triviality".to_owned());
        assert_eq!(page.get_pageid().await.unwrap(), "4138548".to_owned());
        let page = wikipedia.page_from_title("Bikeshedding".to_owned());
        assert_eq!(page.get_pageid().await.unwrap(), "4138548".to_owned());
        let page = wikipedia.page_from_pageid("4138548".to_owned());
        assert_eq!(page.get_pageid().await.unwrap(), "4138548".to_owned());
    }

    #[tokio::test]
    async fn page_html_content() {
        let wikipedia = w();
        let page = wikipedia.page_from_pageid("4138548".to_owned());
        let html = page.get_html_content().await.unwrap();
        assert!(html.contains("bike-shedding"));
        assert!(html.contains("</div>")); // it would not be html otherwise
    }

    #[tokio::test]
    async fn page_summary() {
        let wikipedia = w();
        let page = wikipedia.page_from_title("Parkinson's law of triviality".to_owned());
        let summary = page.get_summary().await.unwrap();
        let content = page.get_content().await.unwrap();
        assert!(summary.contains("bike-shedding"));
        assert!(summary.len() < content.len());
    }

    #[tokio::test]
    async fn page_redirect_summary() {
        let wikipedia = w();
        let page = wikipedia.page_from_title("Bikeshedding".to_owned());
        let summary = page.get_summary().await.unwrap();
        let content = page.get_content().await.unwrap();
        assert!(summary.contains("bike-shedding"));
        assert!(summary.len() < content.len());
    }

    #[tokio::test]
    async fn page_images() {
        let mut wikipedia = w();
        wikipedia.images_results = "5".to_owned();
        let page = wikipedia.page_from_title("Argentina".to_owned());
        let mut images = page.get_images().await.unwrap();
        let mut c = 0;
        let mut set = HashSet::new();
        images.for_each_interrupted(|i| {
            assert!(i.title.len() > 0);
            assert!(i.url.len() > 0);
            assert!(i.description_url.len() > 0);
            c += 1;
            set.insert(i.title);
            if c == 11 {
                return None;
            }
            Some(())
        }).await;
        assert_eq!(set.len(), 11);
    }

    #[tokio::test]
    async fn coordinates() {
        let wikipedia = w();
        let page = wikipedia.page_from_title("San Francisco".to_owned());
        let (lat, lon) = page.get_coordinates().await.unwrap().unwrap();
        assert!(lat > 0.0);
        assert!(lon < 0.0);
    }

    #[tokio::test]
    async fn no_coordinates() {
        let wikipedia = w();
        let page = wikipedia.page_from_title("Bikeshedding".to_owned());
        assert!(page.get_coordinates().await.unwrap().is_none());
    }

    #[tokio::test]
    async fn references() {
        let mut wikipedia = w();
        wikipedia.links_results = "3".to_owned();
        let page = wikipedia.page_from_title("Argentina".to_owned());
        let mut references = page.get_references().await.unwrap();
        let mut c = 0;
        let mut set = HashSet::new();
        references.for_each_interrupted(|r| {
            assert!(r.url.starts_with("http"));
            c += 1;
            set.insert(r.url);
            if c == 7 {
                return None;
            }
            Some(())
        }).await;
        assert_eq!(set.len(), 7);
    }

    #[tokio::test]
    async fn links() {
        let mut wikipedia = w();
        wikipedia.links_results = "3".to_owned();
        let page = wikipedia.page_from_title("Argentina".to_owned());
        let mut links = page.get_links().await.unwrap();
        let mut c = 0;
        let mut set = HashSet::new();
        links.for_each_interrupted(|r| {
            c += 1;
            set.insert(r.title);
            if c == 7 {
                return None;
            }
            Some(())
        }).await;
        assert_eq!(set.len(), 7);
    }

    #[tokio::test]
    async fn langlinks() {
        let mut wikipedia = w();
        wikipedia.links_results = "3".to_owned();
        let page = wikipedia.page_from_title("Law of triviality".to_owned());
        let langlinks = page.get_langlinks().await.unwrap().collect_vec::<Vec<_>>().await;
        assert_eq!(
            langlinks
                .iter()
                .filter(|ll| ll.lang == "nl".to_owned())
                .next()
                .unwrap()
                .title,
            Some("Trivialiteitswet van Parkinson".into()),
        );
        assert_eq!(
            langlinks
                .iter()
                .filter(|ll| ll.lang == "fr".to_owned())
                .next()
                .unwrap()
                .title,
            Some("Loi de futilité de Parkinson".into()),
        );
    }

    #[tokio::test]
    async fn categories() {
        let mut wikipedia = w();
        wikipedia.categories_results = "3".to_owned();
        let page = wikipedia.page_from_title("Argentina".to_owned());
        let mut categories = page.get_links().await.unwrap();
        let mut c = 0;
        let mut set = HashSet::new();
        categories.for_each_interrupted(|ca| {
            c += 1;
            set.insert(ca.title);
            if c == 7 {
                return None;
            }
            Some(())
        }).await;
        assert_eq!(set.len(), 7);
    }

    #[tokio::test]
    async fn sections() {
        let wikipedia = w();
        let page = wikipedia.page_from_title("Bikeshedding".to_owned());
        assert_eq!(
                page.get_sections().await.unwrap(),
                vec![
                "Argument".to_owned(),
                "Examples".to_owned(),
                "Related principles and formulations".to_owned(),
                "See also".to_owned(),
                "References".to_owned(),
                "Further reading".to_owned(),
                "External links".to_owned(),
                ]
                )
    }

    #[tokio::test]
    async fn sections2() {
        let wikipedia = w();
        let page = wikipedia.page_from_pageid("4138548".to_owned());
        assert_eq!(
                page.get_sections().await.unwrap(),
                vec![
                "Argument".to_owned(),
                "Examples".to_owned(),
                "Related principles and formulations".to_owned(),
                "See also".to_owned(),
                "References".to_owned(),
                "Further reading".to_owned(),
                "External links".to_owned(),
                ]
                )
    }

    #[tokio::test]
    async fn section_content() {
        let wikipedia = w();
        let page = wikipedia.page_from_pageid("4138548".to_owned());
        assert!(page.get_section_content("Examples").await.unwrap().unwrap()
                .contains("finance committee meeting"))
    }

    #[tokio::test]
    async fn languages() {
        let languages = w().get_languages().await.unwrap();
        assert!(languages.contains(&("en".to_owned(), "English".to_owned())));
        assert!(languages.contains(&("es".to_owned(), "español".to_owned())));
    }
}

#[cfg(feature = "http-client")]
mod wasm_tests {
    use wikipedia_wasm::Wikipedia;
    use wikipedia_wasm::http;
    use std::collections::HashSet;

    use wasm_bindgen_test::wasm_bindgen_test;

    use crate::wikipedia_wasm::iter::AsyncIterator;

    fn w() -> Wikipedia<http::default::Client> {
        Wikipedia::default()
    }

    #[wasm_bindgen_test]
    async fn search() {
        let wikipedia = w();
        let results = wikipedia.search("hello world").await.unwrap();
        assert!(results.len() > 0);
        assert!(results.contains(&"\"Hello, World!\" program".to_owned()));
    }

    #[wasm_bindgen_test]
    async fn geosearch() {
        let wikipedia = w();
        let results = wikipedia.geosearch(-34.603333, -58.381667, 10).await.unwrap();
        assert!(results.len() > 0);
        assert!(results.contains(&"Buenos Aires".to_owned()));
    }

    #[wasm_bindgen_test]
    async fn random() {
        let wikipedia = w();
        wikipedia.random().await.unwrap().unwrap();
    }

    #[wasm_bindgen_test]
    async fn random_count() {
        let wikipedia = w();
        assert_eq!(wikipedia.random_count(3).await.unwrap().len(), 3);
    }

    #[wasm_bindgen_test]
    async fn page_content() {
        let wikipedia = w();
        let page = wikipedia.page_from_title("Parkinson's law of triviality".to_owned());
        assert!(page.get_content().await.unwrap().contains("bike-shedding"));
    }

    #[wasm_bindgen_test]
    async fn title() {
        let wikipedia = w();
        let page = wikipedia.page_from_title("Parkinson's law of triviality".to_owned());
        assert_eq!(page.get_title().await.unwrap(), "Parkinson's law of triviality".to_owned());
        let page = wikipedia.page_from_pageid("4138548".to_owned());
        assert_eq!(page.get_title().await.unwrap(), "Law of triviality".to_owned());
    }

    #[wasm_bindgen_test]
    async fn pageid() {
        let wikipedia = w();
        let page = wikipedia.page_from_title("Parkinson's law of triviality".to_owned());
        assert_eq!(page.get_pageid().await.unwrap(), "4138548".to_owned());
        let page = wikipedia.page_from_title("Bikeshedding".to_owned());
        assert_eq!(page.get_pageid().await.unwrap(), "4138548".to_owned());
        let page = wikipedia.page_from_pageid("4138548".to_owned());
        assert_eq!(page.get_pageid().await.unwrap(), "4138548".to_owned());
    }

    #[wasm_bindgen_test]
    async fn page_html_content() {
        let wikipedia = w();
        let page = wikipedia.page_from_pageid("4138548".to_owned());
        let html = page.get_html_content().await.unwrap();
        assert!(html.contains("bike-shedding"));
        assert!(html.contains("</div>")); // it would not be html otherwise
    }

    #[wasm_bindgen_test]
    async fn page_summary() {
        let wikipedia = w();
        let page = wikipedia.page_from_title("Parkinson's law of triviality".to_owned());
        let summary = page.get_summary().await.unwrap();
        let content = page.get_content().await.unwrap();
        assert!(summary.contains("bike-shedding"));
        assert!(summary.len() < content.len());
    }

    #[wasm_bindgen_test]
    async fn page_redirect_summary() {
        let wikipedia = w();
        let page = wikipedia.page_from_title("Bikeshedding".to_owned());
        let summary = page.get_summary().await.unwrap();
        let content = page.get_content().await.unwrap();
        assert!(summary.contains("bike-shedding"));
        assert!(summary.len() < content.len());
    }

    #[wasm_bindgen_test]
    async fn page_images() {
        let mut wikipedia = w();
        wikipedia.images_results = "5".to_owned();
        let page = wikipedia.page_from_title("Argentina".to_owned());
        let mut images = page.get_images().await.unwrap();
        let mut c = 0;
        let mut set = HashSet::new();
        images.for_each_interrupted(|i| {
            assert!(i.title.len() > 0);
            assert!(i.url.len() > 0);
            assert!(i.description_url.len() > 0);
            c += 1;
            set.insert(i.title);
            if c == 11 {
                return None;
            }
            Some(())
        }).await;
        assert_eq!(set.len(), 11);
    }

    #[wasm_bindgen_test]
    async fn coordinates() {
        let wikipedia = w();
        let page = wikipedia.page_from_title("San Francisco".to_owned());
        let (lat, lon) = page.get_coordinates().await.unwrap().unwrap();
        assert!(lat > 0.0);
        assert!(lon < 0.0);
    }

    #[wasm_bindgen_test]
    async fn no_coordinates() {
        let wikipedia = w();
        let page = wikipedia.page_from_title("Bikeshedding".to_owned());
        assert!(page.get_coordinates().await.unwrap().is_none());
    }

    #[wasm_bindgen_test]
    async fn references() {
        let mut wikipedia = w();
        wikipedia.links_results = "3".to_owned();
        let page = wikipedia.page_from_title("Argentina".to_owned());
        let mut references = page.get_references().await.unwrap();
        let mut c = 0;
        let mut set = HashSet::new();
        references.for_each_interrupted(|r| {
            assert!(r.url.starts_with("http"));
            c += 1;
            set.insert(r.url);
            if c == 7 {
                return None;
            }
            Some(())
        }).await;
        assert_eq!(set.len(), 7);
    }

    #[wasm_bindgen_test]
    async fn links() {
        let mut wikipedia = w();
        wikipedia.links_results = "3".to_owned();
        let page = wikipedia.page_from_title("Argentina".to_owned());
        let mut links = page.get_links().await.unwrap();
        let mut c = 0;
        let mut set = HashSet::new();
        links.for_each_interrupted(|r| {
            c += 1;
            set.insert(r.title);
            if c == 7 {
                return None;
            }
            Some(())
        }).await;
        assert_eq!(set.len(), 7);
    }

    #[wasm_bindgen_test]
    async fn langlinks() {
        let mut wikipedia = w();
        wikipedia.links_results = "3".to_owned();
        let page = wikipedia.page_from_title("Law of triviality".to_owned());
        let langlinks = page.get_langlinks().await.unwrap().collect_vec::<Vec<_>>().await;
        assert_eq!(
            langlinks
                .iter()
                .filter(|ll| ll.lang == "nl".to_owned())
                .next()
                .unwrap()
                .title,
            Some("Trivialiteitswet van Parkinson".into()),
        );
        assert_eq!(
            langlinks
                .iter()
                .filter(|ll| ll.lang == "fr".to_owned())
                .next()
                .unwrap()
                .title,
            Some("Loi de futilité de Parkinson".into()),
        );
    }

    #[wasm_bindgen_test]
    async fn categories() {
        let mut wikipedia = w();
        wikipedia.categories_results = "3".to_owned();
        let page = wikipedia.page_from_title("Argentina".to_owned());
        let mut categories = page.get_links().await.unwrap();
        let mut c = 0;
        let mut set = HashSet::new();
        categories.for_each_interrupted(|ca| {
            c += 1;
            set.insert(ca.title);
            if c == 7 {
                return None;
            }
            Some(())
        }).await;
        assert_eq!(set.len(), 7);
    }

    #[wasm_bindgen_test]
    async fn sections() {
        let wikipedia = w();
        let page = wikipedia.page_from_title("Bikeshedding".to_owned());
        assert_eq!(
            page.get_sections().await.unwrap(),
            vec![
                "Argument".to_owned(),
                "Examples".to_owned(),
                "Related principles and formulations".to_owned(),
                "See also".to_owned(),
                "References".to_owned(),
                "Further reading".to_owned(),
                "External links".to_owned(),
            ]
        )
    }

    #[wasm_bindgen_test]
    async fn sections2() {
        let wikipedia = w();
        let page = wikipedia.page_from_pageid("4138548".to_owned());
        assert_eq!(
            page.get_sections().await.unwrap(),
            vec![
                "Argument".to_owned(),
                "Examples".to_owned(),
                "Related principles and formulations".to_owned(),
                "See also".to_owned(),
                "References".to_owned(),
                "Further reading".to_owned(),
                "External links".to_owned(),
            ]
        )
    }

    #[wasm_bindgen_test]
    async fn section_content() {
        let wikipedia = w();
        let page = wikipedia.page_from_pageid("4138548".to_owned());
        assert!(page.get_section_content("Examples").await.unwrap().unwrap()
                    .contains("finance committee meeting"))
    }

    #[wasm_bindgen_test]
    async fn languages() {
        let languages = w().get_languages().await.unwrap();
        assert!(languages.contains(&("en".to_owned(), "English".to_owned())));
        assert!(languages.contains(&("es".to_owned(), "español".to_owned())));
    }
}
