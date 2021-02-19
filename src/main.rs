use async_std::task;
use futures::prelude::*;
use serde;
use task::JoinHandle;
use tide::{sse, Request};
use twitter_stream::Token;
use uuid::Uuid;
// use std::time::Duration;
use broadcaster::BroadcastChannel;
use serde::de;
use serde::{Deserialize, Serialize};
use tide::prelude::json;

#[derive(Debug, serde::Deserialize)]
pub struct RequestBody {
    topics: Vec<String>,
}

#[derive(Clone, Debug)]
struct State {
    broadcaster: BroadcastChannel<Tweet>,
}

#[derive(Deserialize)]
#[serde(untagged)]
enum StreamMessage {
    Tweet(Tweet),
    Other(de::IgnoredAny),
}

#[derive(Deserialize, Serialize, Clone, Debug)]
struct Tweet {
    id: u64,
    text: String,
    user: User,
    timestamp_ms: String,
}

#[derive(Deserialize, Serialize, Clone, Debug)]
struct User {
    id: u64,
    screen_name: String,
    profile_image_url_https: String,
}

async fn spawn_tracker(broadcaster: BroadcastChannel<Tweet>) {
    let token = Token::from_parts(
        std::env::var("TW_CONSUMER_KEY").expect("missing env var TW_CONSUMER_KEY"),
        std::env::var("TW_CONSUMER_SECRET").expect("missing env var TW_CONSUMER_SECRET"),
        std::env::var("TW_TOKEN").expect("missing env var TW_TOKEN"),
        std::env::var("TW_SECRET").expect("missing env var TW_SECRET"),
    );

    task::spawn(async move {
        let mut tracker = twitter_stream::Builder::new(token.as_ref());
        let mut stream = tracker
            .track("@_httprs,@rustlang,rust,async-std,rust_foundation")
            .listen()
            .try_flatten_stream();

        while let Some(json) = stream.next().await {
            // let tw: Tweet = serde_json::from_str(&json.unwrap()).unwrap();
            // println!("receive a  tweet! ... , {}", tw.text);
            //     match broadcaster.send(&tw ).await {
            //         Ok(_) => {},
            //         Err(_) => { println!("Error sending to broadcaster") }
            //     };

            if let Ok(StreamMessage::Tweet(tw)) = serde_json::from_str(&json.unwrap()) {
                println!("receive a  tweet! ... , {}", tw.text);
                match broadcaster.send(&tw).await {
                    Ok(_) => {}
                    Err(_) => {
                        println!("Error sending to broadcaster")
                    }
                };
            }
        }
    });
}
#[async_std::main]
async fn main() -> Result<(), std::io::Error> {
    dotenv::dotenv().ok();

    tide::log::start();

    let broadcaster = BroadcastChannel::new();

    // spawn tracker
    spawn_tracker(broadcaster.clone()).await;

    let mut app = tide::with_state(State { broadcaster });

    // serve public dir for assets
    app.at("/public").serve_dir("./public/")?;

    // index route
    app.at("/").serve_file("public/index.html")?;

    // // view route
    // app.at("/:track_id").serve_file("public/stream.html")?;

    // sse route
    app.at("/sse")
        .get(sse::endpoint(|req: Request<State>, sender| async move {
            let state = req.state().clone();
            while let Some(tweet) = state.broadcaster.clone().next().await {
                println!("sending tweet");
                println!("{:?}", tweet);
                sender.send("tweet", json!(tweet).to_string(), None).await?;
            }

            Ok(())
        }));

    app.at("/track").post(|mut req: Request<State>| async move {
        let body_string = req.body_string().await?;
        let payload = percent_encoding::percent_decode(body_string.as_bytes())
            .decode_utf8()
            .unwrap();
        let body: RequestBody = serde_qs::from_str(&payload).unwrap();

        println!("{:?}", body);
        let track_id = Uuid::new_v4();
        Ok(tide::Redirect::see_other(format!("/{}", track_id)))
    });

    let port = std::env::var("PORT").unwrap_or_else(|_| "8080".to_string());
    let addr = format!("0.0.0.0:{}", port);
    app.listen(addr).await?;

    Ok(())
}
