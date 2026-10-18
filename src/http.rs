//! Minimal HTTPS GET on top of WinHTTP (ships with Windows: no TLS library to bundle, honours system proxy/certs).

use std::ptr::{null, null_mut};
use windows_sys::Win32::Networking::WinHttp::*;

const UA: &str = "wxscr/0.1 (+https://github.com/Jesse-Hufstetler/wxscr)";

fn w(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(Some(0)).collect()
}

struct Handle(*mut core::ffi::c_void);
impl Drop for Handle {
    fn drop(&mut self) {
        if !self.0.is_null() {
            unsafe { WinHttpCloseHandle(self.0) };
        }
    }
}

pub fn get(url: &str) -> Result<String, String> {
    let (https, rest) = if let Some(r) = url.strip_prefix("https://") {
        (true, r)
    } else if let Some(r) = url.strip_prefix("http://") {
        (false, r)
    } else {
        return Err(format!("bad url {url}"));
    };
    let (host, path) = match rest.find('/') {
        Some(i) => (&rest[..i], &rest[i..]),
        None => (rest, "/"),
    };
    unsafe {
        let session = Handle(WinHttpOpen(w(UA).as_ptr(), WINHTTP_ACCESS_TYPE_AUTOMATIC_PROXY, null(), null(), 0));
        if session.0.is_null() {
            return Err("WinHttpOpen failed".into());
        }
        WinHttpSetTimeouts(session.0, 8000, 8000, 15000, 25000);
        let port = if https { 443 } else { 80 };
        let conn = Handle(WinHttpConnect(session.0, w(host).as_ptr(), port, 0));
        if conn.0.is_null() {
            return Err(format!("connect {host} failed"));
        }
        let flags = if https { WINHTTP_FLAG_SECURE } else { 0 };
        let req = Handle(WinHttpOpenRequest(conn.0, w("GET").as_ptr(), w(path).as_ptr(), null(), null(), null_mut(), flags));
        if req.0.is_null() {
            return Err("open request failed".into());
        }
        if WinHttpSendRequest(req.0, null(), 0, null(), 0, 0, 0) == 0 || WinHttpReceiveResponse(req.0, null_mut()) == 0 {
            return Err(format!("request to {host} failed"));
        }
        let mut status: u32 = 0;
        let mut len = 4u32;
        WinHttpQueryHeaders(
            req.0,
            WINHTTP_QUERY_STATUS_CODE | WINHTTP_QUERY_FLAG_NUMBER,
            null(),
            &mut status as *mut u32 as *mut _,
            &mut len,
            null_mut(),
        );
        let mut body = Vec::new();
        let mut buf = [0u8; 16384];
        loop {
            let mut n = 0u32;
            if WinHttpReadData(req.0, buf.as_mut_ptr() as *mut _, buf.len() as u32, &mut n) == 0 || n == 0 {
                break;
            }
            body.extend_from_slice(&buf[..n as usize]);
        }
        if status != 200 {
            return Err(format!("HTTP {status} from {host}"));
        }
        Ok(String::from_utf8_lossy(&body).into_owned())
    }
}

pub fn get_json(url: &str) -> Result<serde_json::Value, String> {
    let mut last = String::new();
    for attempt in 0..2 {
        match get(url).and_then(|s| serde_json::from_str(&s).map_err(|e| e.to_string())) {
            Ok(v) => return Ok(v),
            Err(e) => last = e,
        }
        if attempt == 0 {
            std::thread::sleep(std::time::Duration::from_millis(1500));
        }
    }
    Err(last)
}
