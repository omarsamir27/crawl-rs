use isahc::http::{HeaderMap, HeaderValue, StatusCode, Version};
use reqwest::Response as Resp;
use soup::Soup;
use std::net::IpAddr;
use warc::{BufferedBody, RawRecordHeader, Record, RecordType, WarcHeader};

use crate::crawl_utils;

pub struct WetRecord {
    pub headers: RawRecordHeader,
    pub body: String,
}

pub struct Response {
    ip: String,
    version: String,
    status: String,
    url: String,
    data: String,
    content_length: u64,
    headers: String,
    time: String,
}

impl Response {
    pub fn new(
        ip: IpAddr,
        version: Version,
        status: StatusCode,
        url: &str,
        data: &str,
        content_length: u64,
        headers: &HeaderMap,
        time: &str,
    ) -> Self {
        let mut headers = headers.clone();
        headers.insert("content-length", HeaderValue::from(content_length));
        let headers = crawl_utils::http_headers_fmt(&headers);
        Response {
            ip: ip.to_string(),
            version: format!("{version:?}"),
            status: status.to_string(),
            url: url.to_string(),
            data: data.to_string(),
            content_length,
            headers,
            time: time.to_string(),
        }
    }
    pub fn to_soup(&self) -> Soup {
        return Soup::new(self.data.as_str());
    }

    pub fn to_warcrecord(&self, soup: Option<&Soup>) -> WetRecord {
        let text = match soup {
            Some(my_soup) => my_soup.text(),
            None => self.to_soup().text(),
        };
        let datetime = self.time.as_bytes().to_vec();
        let ip = self.ip.as_bytes().to_vec();
        let headers = RawRecordHeader {
            version: "1.0".to_owned(),
            headers: vec![
                (
                    WarcHeader::RecordID,
                    Record::<BufferedBody>::generate_record_id().into_bytes(),
                ),
                (WarcHeader::TargetURI,
                    self.url.clone().into_bytes()
                ),
                (
                    WarcHeader::WarcType,
                    RecordType::WarcInfo.to_string().into_bytes(),
                ),
                (WarcHeader::Date, datetime),
                (WarcHeader::IPAddress, ip),
                (
                    WarcHeader::ContentLength,
                    text.len().to_string().into_bytes(),
                ),
            ]
            .into_iter()
            .collect(),
        };
        WetRecord {
            headers,
            body: String::from(text),
        }
    }
    pub async fn from_request(resp: Resp) -> Result<Self, ResponseError> {
        let content_length: u64 = match resp.content_length() {
            None => 0,
            Some(length) => length,
        };

        let headers = resp.headers().clone();
        let ip = resp.remote_addr().unwrap().ip();
        let version = resp.version();
        let status = resp.status();
        let url = resp.url().clone();
        let text = resp.text().await;
        if text.is_err() {
            return Err(ResponseError::TextError);
        }
        let text = text.unwrap();
        let time = chrono::Local::now().to_string();

        let response = Response::new(
            ip,
            version,
            status,
            url.as_str(),
            text.as_str(),
            content_length,
            &headers,
            time.as_str(),
        );
        Ok(response)
    }
}

// impl From<Resp> for Response {
//     async fn from(resp: Resp) -> Result<Self,Err(ResponseError)> {
//
//
//     }
// }

#[derive(Debug)]
pub enum ResponseError {
    RequestError,
    TextError,
}
