use display_as::{display, format_as, with_template, DisplayAs, HTML, UTF8, URL};
use serde::Deserialize;
use warp::{path, Filter};
use auto_args::AutoArgs;

#[derive(Clone)]
struct Index {
    args: Args,
}
#[with_template("[%" "%]" "index.html")]
impl DisplayAs<HTML> for Index {}
struct Overview {
    board: String,
    n: usize,
    args: Args,
}
#[with_template("[%" "%]" "overview.html")]
impl DisplayAs<HTML> for Overview {}

use warp::Reply;
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
struct Student {
    name: String,
    email: String,
    friends: Vec<String>,
    enemies: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
struct Group {
    num: usize,
    name: String,
    students: Vec<Student>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
struct Groups {
    title: String,
    board: String,
    min_students: usize,
    students: Vec<Student>,
    absent: Vec<Student>,
    instructors: Vec<Student>,
    groups: Vec<Group>,
    #[serde(skip)]
    args: Args,
}
#[with_template("[%" "%]" "groups.html")]
impl DisplayAs<HTML> for Groups {}

lazy_static::lazy_static! {
  static ref ZOOM: std::sync::Mutex<Option<String>> = std::sync::Mutex::new(None);
}

struct ExampleCSV;
#[with_template("[%" "%]" "example.csv")]
impl DisplayAs<UTF8> for ExampleCSV {}

#[derive(Clone, Debug, PartialEq, Eq, AutoArgs, Default)]
struct Args {
    /// The directory in which to find wbo running
    board: Option<String>,
    /// The host this is running on
    host: Option<String>,
}
impl Args {
    fn board_directory<'a>(&'a self) -> impl DisplayAs<URL> + 'a {
        if let Some(url) = &self.board {
            url.as_str()
        } else {
            "wbo"
        }
    }
    fn board_url<'a>(&'a self) -> impl DisplayAs<URL> + 'a {
        format_as!(URL, self.host_url() "/" self.board_directory())
    }
    fn host_url<'a>(&'a self) -> impl DisplayAs<URL> + 'a {
        if let Some(url) = &self.host {
            url.as_str()
        } else {
            "https://bingley.physics.oregonstate.edu"
        }
    }
    fn overview_url<'a>(&'a self) -> impl DisplayAs<URL> + 'a {
        format_as!(URL, self.host_url() "/overview")
    }
}

#[tokio::main]
async fn main() {
    let args_original = Args::from_args();
    pretty_env_logger::init();

    let example = warp::path!("example.csv")
        .map(move || display(UTF8, &ExampleCSV).into_response());

    let args = args_original.clone();
    let overview = warp::path!(String / usize)
        .map(move |board, n| display(HTML, &Overview { board, n, args: args.clone() }).into_response());

    let zoom = warp::path!("zoom.csv").map(move || {
        let s = if let Some(s) = &*ZOOM.lock().unwrap() {
            s.clone()
        } else {
            "You can only download this file once per set of groups.".to_string()
        };
        *ZOOM.lock().unwrap() = None;
        s
    });

    let args = args_original.clone();
    use bytes::BufMut;
    use futures::{TryFutureExt, TryStreamExt};
    let submit = warp::path!("submit")
        .and(warp::multipart::form())
        .and_then(|form: warp::multipart::FormData| {
            async {
                // Collect the fields into (name, value): (String, Vec<u8>)
                let part: Result<Vec<(String, Vec<u8>)>, warp::Rejection> = form
                    .and_then(|part| {
                        let name = part.name().to_string();
                        let value = part.stream().try_fold(Vec::new(), |mut vec, data| {
                            vec.put(data);
                            async move { Ok(vec) }
                        });
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
        .map(move |x: Vec<(String, Vec<u8>)>| {
            for (a, b) in x.iter() {
                if a == "csv" {
                    let mut data = Groups {
                        title: "".to_string(),
                        board: memorable_wordlist::camel_case(40),
                        min_students: 3,
                        students: Vec::new(),
                        instructors: Vec::new(),
                        absent: Vec::new(),
                        groups: Vec::new(),
                        args: args.clone(),
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
                                data.board = format!("{}-{}", slug::slugify(&data.title), memorable_wordlist::camel_case(30));
                                println!("setting board to {}", data.board);
                            } else if x[0] == "minimum" {
                                if let Ok(m) = x[1].parse::<usize>() {
                                    data.min_students = m;
                                }
                            } else if x[1] != "" {
                                // println!("{}  ->  {}", x[0], x[1]);
                                let mut student = Student {
                                    name: x[0].to_string(),
                                    email: x[1].to_string(),
                                    friends: Vec::new(),
                                    enemies: Vec::new(),
                                };
                                if x.len() > 2 {
                                    for o in x[2..].iter().filter(|o| o.len() > 1) {
                                        match o.split_at(1) {
                                            ("-", name) => student.enemies.push(name.to_string()),
                                            ("+", name) => student.friends.push(name.to_string()),
                                            _ => (),
                                        }
                                    }
                                }
                                if x.len() < 3 || !["absent", "instructor"].contains(&x[2].as_ref()) {
                                    data.students.push(student);
                                } else if x[2] == "instructor" {
                                    data.instructors.push(student);
                                } else {
                                    data.absent.push(student);
                                }
                            }
                        }
                    }
                    let base = data.board.clone();
                    let mut rng = rand::thread_rng();
                    let mut students_left = data.students.clone();
                    use rand::seq::SliceRandom;
                    students_left.shuffle(&mut rng);
                    let num_groups = data.students.len() / (data.min_students + 1);
                    for groupnum in 1..=num_groups + 1 {
                        data.groups.push(Group {
                            num: groupnum,
                            name: format!("{}-{}", groupnum, base),
                            students: Vec::new(),
                        })
                    }
                    while students_left.len() > 0 {
                        let s = students_left.pop().unwrap();
                        let spots_less_than_min = data
                            .groups
                            .iter()
                            .map(|g| {
                                if g.students.len() > data.min_students {
                                    0
                                } else {
                                    data.min_students - g.students.len()
                                }
                            })
                            .count();
                        let min_students = data.min_students;
                        let is_full = |g: &Group| {
                            if spots_less_than_min == students_left.len() + 1 {
                                g.students.len() >= min_students
                            } else {
                                g.students.len() >= min_students + 1
                            }
                        };
                        let badness = |g: &Group| -> i64 {
                            let mut score: i64 = g.students.len() as i64;
                            if is_full(g) {
                                score += 10000;
                            }
                            for x in g.students.iter() {
                                if s.enemies.contains(&x.name) || x.enemies.contains(&s.name) {
                                    score += 100;
                                }
                                if s.friends.contains(&x.name) {
                                    score -= 1;
                                }
                                if x.friends.contains(&s.name) {
                                    score -= 1;
                                }
                            }
                            score
                        };
                        data.groups.sort_by_cached_key(badness);
                        data.groups[0].students.push(s);
                    }
                    let mut students_left = data.absent.clone();
                    students_left.shuffle(&mut rng);
                    while students_left.len() > 0 {
                        let s = students_left.pop().unwrap();
                        let badness = |g: &Group| -> i64 {
                            let mut score: i64 = 1000*g.students.len() as i64;
                            for x in g.students.iter() {
                                if s.enemies.contains(&x.name) || x.enemies.contains(&s.name) {
                                    score += 100;
                                }
                                if s.friends.contains(&x.name) {
                                    score -= 1;
                                }
                                if x.friends.contains(&s.name) {
                                    score -= 1;
                                }
                            }
                            score
                        };
                        data.groups.sort_by_cached_key(badness);
                        data.groups[0].students.push(s);
                    }
                    for g in data.groups.iter_mut() {
                        g.students.shuffle(&mut rng);
                    }
                    data.groups.sort_by_cached_key(|g| g.num);
                    use std::fmt::Write;
                    let mut zoom = String::new();
                    writeln!(&mut zoom, "Pre-assign Room Name,Email Address").ok();
                    for g in data.groups.iter_mut() {
                        g.students.shuffle(&mut rng);
                        for s in g.students.iter() {
                            writeln!(&mut zoom, "{},{}", g.name, s.email).ok();
                        }
                    }
                    for i in data.instructors.iter() {
                        writeln!(&mut zoom, "{},{}", "teaching team", i.email).ok();
                    }
                    *ZOOM.lock().unwrap() = Some(zoom);
                    return display(HTML, &data).into_response();
                }
            }
            return display(HTML, &"Error parsing CSV file".to_string()).into_response();
        })
        .with(warp::log("foo"));

    let my_index = Index { args: args_original.clone() };
    let index = warp::path::end()
        .or(path!("index.html"))
        .map(move |_| display(HTML, &my_index).into_response());

    warp::serve(zoom.or(example).or(overview).or(submit).or(index))
        .run(([127, 0, 0, 1], 3030))
        .await;
}
