#![allow(async_fn_in_trait)]
pub use failure::Error;

pub trait HttpClient {
    fn user_agent(&mut self, user_agent: String);
    async fn get<'a, I>(&self, base_url: &str, args: I) -> Result<String, Error>
    where
        I: Iterator<Item = (&'a str, &'a str)>;
}


#[cfg(feature = "http-client")]
pub mod default {
    use failure::err_msg;
    use reqwest;

    use super::{Error, HttpClient};

    pub struct Client {
        user_agent: String,
    }

    impl Default for Client {
        fn default() -> Self {
            Client {
                user_agent: "".to_owned(),
            }
        }
    }

    impl HttpClient for Client {
        fn user_agent(&mut self, user_agent: String) {
            self.user_agent = user_agent;
        }

        async fn get<'a, I>(&self, base_url: &str, args: I) -> Result<String, Error>
        where
            I: Iterator<Item = (&'a str, &'a str)>,
        {
            let url = reqwest::Url::parse_with_params(base_url, args)?;

            let client = reqwest::Client::new();
            let response = client
                .get(url)
                .header(reqwest::header::USER_AGENT, self.user_agent.clone())
                .send().await?;

            ensure!(response.status().is_success(), err_msg("Bad status"));

            let response_str = response.text().await?;

            Ok(response_str)
        }
    }
}