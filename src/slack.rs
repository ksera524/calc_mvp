use reqwest::header::{HeaderMap, AUTHORIZATION, CONTENT_TYPE};
use serde::Serialize;
use dotenv::dotenv;
use std::error::Error;

#[derive(Serialize)]
struct Massage {
    channel: String,
    text: String,
}

pub async fn send_slack_message(message: &str) -> Result<(), Box<dyn Error>> {
    dotenv().ok();
    let client = reqwest::Client::new();
    let url = "https://slack.com/api/chat.postMessage";

    let token = std::env::var("TOKEN")?;
    let channel = std::env::var("CHANNEL")?;

    let mut headers = HeaderMap::new();
    headers.insert(AUTHORIZATION, format!("Bearer {}", token).parse()?);
    headers.insert(CONTENT_TYPE, "application/json".parse()?);

    let data = Massage {
        channel,
        text: message.to_string(),
    };

    let response = client.post(url)
        .headers(headers)
        .body(serde_json::to_string(&data).unwrap())
        .send()
        .await?;

    if response.status().is_success() {
        println!("Message sent successfully");
    } else {
        println!("Error: {}", response.status());
        // Optionally, print the response body for more details on the error
        let error_text = response.text().await?;
        println!("{}", error_text);
    }

    Ok(())
}