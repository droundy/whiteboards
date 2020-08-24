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
struct Student {
    name: String,
    email: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
struct Group {
    name: String,
    students: Vec<Student>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
struct Groups {
    title: String,
    min_students: usize,
    students: Vec<Student>,
    absent: Vec<Student>,
    groups: Vec<Group>,
}
#[with_template("[%" "%]" "groups.html")]
impl DisplayAs<HTML> for Groups {}

lazy_static::lazy_static! {
  static ref ZOOM: std::sync::Mutex<Option<String>> = std::sync::Mutex::new(None);
}

#[tokio::main]
async fn main() {
    pretty_env_logger::init();

    let zoom = warp::path!("zoom.csv").map(move || {
        let s = if let Some(s) = &*ZOOM.lock().unwrap() {
            s.clone()
        } else {
            "You can only download this file once per set of groups.".to_string()
        };
        *ZOOM.lock().unwrap() = None;
        s
    });

    use bytes::BufMut;
    use futures::{TryFutureExt, TryStreamExt};
    let submit = warp::path!("submit")
        .and(warp::multipart::form())
        .and_then(|form: warp::multipart::FormData| {
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
        .map(|x: Vec<(String, Vec<u8>)>| {
            for (a, b) in x.iter() {
                if a == "csv" {
                    println!("got some good {:?}", String::from_utf8_lossy(b));

                    let mut data = Groups {
                        title: "".to_string(),
                        min_students: 3,
                        students: Vec::new(),
                        absent: Vec::new(),
                        groups: Vec::new(),
                    };
                    let s = String::from_utf8_lossy(b);
                    let mut rdr = csv::ReaderBuilder::new()
                        .has_headers(false)
                        .from_reader(s.as_bytes());
                    for r in rdr.records() {
                        if let Ok(r) = r {
                            let x: Vec<String> = r.iter().map(|x| x.to_string()).collect();
                            if x[0] == "title" {
                                data.title = x[1].to_string();
                            } else if x[0] == "minimum" {
                                if let Ok(m) = x[1].parse::<usize>() {
                                    data.min_students = m;
                                }
                            } else if x[1] != "" {
                                println!("{}  ->  {}", x[0], x[1]);
                                if x.len() < 3 || x[2] != "absent" {
                                    data.students.push(Student {
                                        name: x[0].to_string(),
                                        email: x[1].to_string(),
                                    });
                                } else {
                                    data.absent.push(Student {
                                        name: x[0].to_string(),
                                        email: x[1].to_string(),
                                    });
                                }
                            }
                        }
                    }
                    let mut rng = rand::thread_rng();
                    let mut students_left = data.students.clone();
                    use rand::seq::SliceRandom;
                    students_left.shuffle(&mut rng);
                    while students_left.len() > 0 {
                        if students_left.len() <= data.min_students + 1 {
                            data.groups.push(Group {
                                name: memorable_wordlist::camel_case(40),
                                students: students_left.drain(..).collect(),
                            })
                        } else if students_left.len() % (data.min_students + 1) == 0 {
                            data.groups.push(Group {
                                name: memorable_wordlist::camel_case(40),
                                students: students_left.drain(0..data.min_students + 1).collect(),
                            })
                        } else {
                            data.groups.push(Group {
                                name: memorable_wordlist::camel_case(40),
                                students: students_left.drain(0..data.min_students).collect(),
                            })
                        }
                    }
                    let mut absent_left = data.absent.clone();
                    absent_left.shuffle(&mut rng);
                    for a in absent_left.into_iter() {
                        if let Some(smallest) = data.groups.iter().map(|g| g.students.len()).min() {
                            data.groups
                                .iter_mut()
                                .filter(|g| g.students.len() == smallest)
                                .next()
                                .unwrap()
                                .students
                                .push(a);
                        }
                    }
                    use std::fmt::Write;
                    let mut zoom = String::new();
                    writeln!(&mut zoom, "\n\nPre-assign Room Name,Email Address").ok();
                    for g in data.groups.iter_mut() {
                        g.students.shuffle(&mut rng);
                        for s in g.students.iter() {
                            writeln!(&mut zoom, "{},{}", g.name, s.email).ok();
                        }
                    }
                    *ZOOM.lock().unwrap() = Some(zoom);
                    return display(HTML, &data).into_response();
                }
            }
            return display(HTML, &"Error parsing CSV file".to_string()).into_response();
        })
        .with(warp::log("foo"));

    let index = warp::path::end()
        .or(path!("index.html"))
        .map(move |_| display(HTML, &Index {}));

    warp::serve(zoom.or(submit).or(index))
        .run(([127, 0, 0, 1], 3030))
        .await;
}
