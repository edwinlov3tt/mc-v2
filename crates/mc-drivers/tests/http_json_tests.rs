//! HTTP/JSON driver tests using an in-process `std::net::TcpListener`
//! mock server. No external dependencies — handwritten HTTP/1.1 response
//! framing on a localhost ephemeral port.

use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::mpsc;
use std::thread;

use mc_drivers::{http_json_driver, ColumnData, ColumnDataType, SourceDriver};

/// Spin up a one-shot HTTP server that responds to the next incoming
/// request with `(status, body)`. Returns the URL the client should use
/// (`http://127.0.0.1:<port>/`).
fn one_shot_server(status: u16, body: String) -> String {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind ephemeral");
    let port = listener.local_addr().expect("local_addr").port();
    let url = format!("http://127.0.0.1:{}/", port);

    let (tx, rx) = mpsc::channel();
    thread::spawn(move || {
        let (mut stream, _addr) = match listener.accept() {
            Ok(p) => p,
            Err(_) => return,
        };
        // Drain request headers (until blank line) so the client can finish.
        let mut reader = BufReader::new(&stream);
        let mut line = String::new();
        loop {
            line.clear();
            if reader.read_line(&mut line).unwrap_or(0) == 0 {
                break;
            }
            if line == "\r\n" || line == "\n" {
                break;
            }
        }
        drop(reader);
        let status_line = match status {
            200 => "HTTP/1.1 200 OK",
            404 => "HTTP/1.1 404 Not Found",
            500 => "HTTP/1.1 500 Internal Server Error",
            _ => "HTTP/1.1 200 OK",
        };
        let response = format!(
            "{}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
            status_line,
            body.len(),
            body
        );
        let _ = stream.write_all(response.as_bytes());
        let _ = stream.flush();
        let _ = stream.shutdown(std::net::Shutdown::Both);
        // Help reader on the other end quiesce.
        let _ = tx.send(());
    });
    // Give the server thread a moment to bind & start listening.
    let _ = rx.recv_timeout(std::time::Duration::from_millis(100));
    url
}

/// Simpler variant: just the URL, server already accepted by spawn time.
fn serve(body: &str) -> String {
    one_shot_server(200, body.to_string())
}

#[test]
fn t_http_json_driver_reads_root_array() {
    let url = serve(
        r#"[
        {"id": 1, "name": "alice", "score": 98.5, "active": true},
        {"id": 2, "name": "bob", "score": 72.0, "active": false},
        {"id": 3, "name": "carol", "score": 85.25, "active": true}
        ]"#,
    );

    let mut d = http_json_driver(&url, None).expect("driver");
    let schema = d.schema().expect("schema");
    assert_eq!(schema.len(), 4);

    let by_name = |n: &str| schema.iter().find(|c| c.name == n).expect(n);
    assert_eq!(by_name("id").data_type, ColumnDataType::F64);
    assert_eq!(by_name("name").data_type, ColumnDataType::Str);
    assert_eq!(by_name("score").data_type, ColumnDataType::F64);
    assert_eq!(by_name("active").data_type, ColumnDataType::Bool);

    let batch = d.fetch_batch(100).unwrap().expect("rows");
    assert_eq!(batch.row_count, 3);
}

#[test]
fn t_http_json_driver_navigates_json_path() {
    let url = serve(
        r#"{"meta": {"page": 1}, "data": {"rows": [
            {"k": 1, "v": "a"},
            {"k": 2, "v": "b"}
        ]}}"#,
    );
    let mut d = http_json_driver(&url, Some("data.rows")).expect("driver");
    let batch = d.fetch_batch(100).unwrap().expect("rows");
    assert_eq!(batch.row_count, 2);
    let kcol = batch.columns.iter().find(|c| c.name == "k").expect("k col");
    if let ColumnData::F64(v) = &kcol.data {
        assert_eq!(v, &vec![Some(1.0), Some(2.0)]);
    } else {
        panic!("k should be F64");
    }
}

#[test]
fn t_http_json_driver_handles_nulls_and_missing_keys() {
    let url = serve(
        r#"[
            {"a": 1, "b": "x"},
            {"a": null, "b": "y", "c": "extra"},
            {"a": 3}
        ]"#,
    );
    let mut d = http_json_driver(&url, None).expect("driver");
    let schema = d.schema().expect("schema");
    let a = schema.iter().find(|c| c.name == "a").expect("a");
    let b = schema.iter().find(|c| c.name == "b").expect("b");
    let c = schema.iter().find(|c| c.name == "c").expect("c");
    assert!(a.nullable, "a has explicit null");
    assert!(b.nullable, "b is missing in row 3");
    assert!(c.nullable, "c is missing in rows 1+2");

    let batch = d.fetch_batch(100).unwrap().expect("rows");
    assert_eq!(batch.row_count, 3);
}

#[test]
fn t_http_json_driver_serializes_nested_objects_as_str() {
    let url = serve(
        r#"[
            {"x": 1, "nested": {"k": "v"}},
            {"x": 2, "nested": [1, 2, 3]}
        ]"#,
    );
    let mut d = http_json_driver(&url, None).expect("driver");
    let schema = d.schema().expect("schema");
    let nested = schema.iter().find(|c| c.name == "nested").expect("nested");
    assert_eq!(nested.data_type, ColumnDataType::Str);
    let batch = d.fetch_batch(100).unwrap().expect("rows");
    let nested_col = batch
        .columns
        .iter()
        .find(|c| c.name == "nested")
        .expect("col");
    if let ColumnData::Str(v) = &nested_col.data {
        assert!(v[0].as_ref().unwrap().contains("\"k\""));
        assert!(v[1].as_ref().unwrap().contains("[1,2,3]"));
    } else {
        panic!("nested should be Str");
    }
}

#[test]
fn t_http_json_driver_http_404_yields_http_status() {
    let url = one_shot_server(404, "not found".to_string());
    let err = http_json_driver(&url, None).expect_err("404 errors");
    let s = format!("{}", err);
    assert!(s.contains("http 404"), "expected HttpStatus 404, got {}", s);
}

#[test]
fn t_http_json_driver_invalid_json_yields_malformed_source() {
    let url = serve("not json {");
    let err = http_json_driver(&url, None).expect_err("invalid json");
    let s = format!("{}", err);
    assert!(
        s.contains("malformed") || s.contains("invalid JSON"),
        "got: {}",
        s
    );
}

#[test]
fn t_http_json_driver_root_not_array_with_no_path_yields_path_error() {
    let url = serve(r#"{"foo": "bar"}"#);
    let err = http_json_driver(&url, None).expect_err("root object, no path");
    let s = format!("{}", err);
    assert!(s.contains("did not select an array"), "got: {}", s);
}

#[test]
fn t_http_json_driver_batches_at_max_rows() {
    let url = serve(r#"[{"x":1},{"x":2},{"x":3},{"x":4},{"x":5}]"#);
    let mut d = http_json_driver(&url, None).expect("driver");
    let b1 = d.fetch_batch(2).unwrap().expect("b1");
    assert_eq!(b1.row_count, 2);
    let b2 = d.fetch_batch(2).unwrap().expect("b2");
    assert_eq!(b2.row_count, 2);
    let b3 = d.fetch_batch(2).unwrap().expect("b3");
    assert_eq!(b3.row_count, 1);
    assert!(d.fetch_batch(2).unwrap().is_none(), "exhausted");
}

#[test]
fn t_http_json_driver_cancel_returns_none() {
    let url = serve(r#"[{"x":1},{"x":2},{"x":3}]"#);
    let mut d = http_json_driver(&url, None).expect("driver");
    d.cancel();
    assert!(d.fetch_batch(10).unwrap().is_none());
}

#[test]
fn t_http_json_driver_connection_refused_yields_connection_failed() {
    // Bind an ephemeral port then drop the listener so port is free,
    // and immediately try to connect — high probability of refused
    // connection without DNS lookup.
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind");
    let port = listener.local_addr().unwrap().port();
    drop(listener);
    let url = format!("http://127.0.0.1:{}/", port);
    let err = http_json_driver(&url, None).expect_err("connection refused");
    let s = format!("{}", err);
    assert!(
        s.contains("connection failed") || s.contains("refused") || s.contains("Connection"),
        "got: {}",
        s
    );
}

#[allow(dead_code)] // referenced via `read_to_string` only when needed
fn drain_response(stream: &mut TcpStream) -> String {
    let mut s = String::new();
    let _ = stream.read_to_string(&mut s);
    s
}
