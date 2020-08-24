use display_as::{display, with_template, DisplayAs, HTML};
use serde::Deserialize;
use warp::{path, Filter};

struct Index {}
#[with_template("[%" "%]" "index.html")]
impl DisplayAs<HTML> for Index {}
struct Overview {
    course_name: String,
}
#[with_template("[%" "%]" "overview.html")]
impl DisplayAs<HTML> for Overview {}

use warp::Reply;

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
struct Submit {
    csv: String,
}

#[tokio::main]
async fn main() {
    // GET /hello/warp => 200 OK with body "Hello, warp!"
    let overview = warp::path!("overview" / String).map(move |name| {
        let o = Overview { course_name: name };
        display(HTML, &o).into_response()
    });

    use bytes::BufMut;
    use futures::{TryFutureExt, TryStreamExt};
    let submit = warp::path!("submit").and(warp::multipart::form()).and_then(|form: warp::multipart::FormData| {
        async {
            // Collect the fields into (name, value): (String, Vec<u8>)
            let part: Result<Vec<(String, Vec<u8>)>, warp::Rejection> = form
                .and_then(|part| {
                    println!("at the start?");
                    let name = part.name().to_string();
                    let value = part.stream().try_fold(Vec::new(), |mut vec, data| {
                        vec.put(data);
                        async move { Ok(vec) }
                    });
                    println!("I am processing in the middle...");
                    value.map_ok(move |vec| (name, vec))
                })
                .try_collect()
                .await
                .map_err(|e| {
                    panic!("multipart error: {:?}", e);
                });
            part
        }
    })
    .map(|x| {
        format!("Hello world {:?}", x)
    });

    // let submit = warp::path!("submit")
    //     .and(warp::body::content_length_limit(1024 * 128))
    //     .and(warp::filters::multipart::form())
    //     .map(move |x: Submit| format!("{:?}", x));
    let index = warp::path::end()
        .or(path!("index.html"))
        .map(move |_| display(HTML, &Index {}));

    warp::serve(overview.or(submit).or(index))
        .run(([127, 0, 0, 1], 3030))
        .await;
}
