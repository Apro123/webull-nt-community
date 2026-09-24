// -------------------------------------------------------------------------------------------------
//  Copyright (C) 2015-2026 Nautech Systems Pty Ltd. All rights reserved.
//  https://nautechsystems.io
//
//  Licensed under the GNU Lesser General Public License Version 3.0 (the "License");
//  You may not use this file except in compliance with the License.
//  You may obtain a copy of the License at https://www.gnu.org/licenses/lgpl-3.0.en.html
//
//  Unless required by applicable law or agreed to in writing, software
//  distributed under the License is distributed on an "AS IS" BASIS,
//  WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
//  See the License for the specific language governing permissions and
//  limitations under the License.
// -------------------------------------------------------------------------------------------------

//! A loopback single-thread HTTP mock server for integration tests.

use std::{
    io::{Read, Write},
    net::TcpListener,
    sync::mpsc,
    thread,
};

/// Serves canned responses in order on a loopback port.
///
/// Returns the base URL and a receiver for the captured raw requests.
#[must_use]
pub(crate) fn start(responses: Vec<(u16, String)>) -> (String, mpsc::Receiver<String>) {
    let listener = TcpListener::bind("127.0.0.1:0").expect("binds loopback port");
    let addr = listener.local_addr().expect("local address");
    let (tx, rx) = mpsc::channel();

    thread::spawn(move || {
        for (status, body) in responses {
            let (mut stream, _) = listener.accept().expect("accepts connection");
            let mut collected = Vec::new();
            let mut chunk = [0u8; 4096];
            loop {
                match stream.read(&mut chunk) {
                    Ok(0) => break,
                    Ok(n) => {
                        collected.extend_from_slice(&chunk[..n]);
                        if String::from_utf8_lossy(&collected).contains("\r\n\r\n") {
                            break;
                        }
                    }
                    Err(_) => break,
                }
            }
            tx.send(String::from_utf8_lossy(&collected).to_string())
                .expect("sends captured request");

            let response = format!(
                "HTTP/1.1 {status} Mock\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            );
            stream
                .write_all(response.as_bytes())
                .expect("writes response");
        }
    });

    (format!("http://127.0.0.1:{}", addr.port()), rx)
}
