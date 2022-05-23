#![allow(dead_code)]

use std::{cell::RefCell, collections::HashMap, fmt::Display, vec};

use chrono::FixedOffset;
use imap_proto::{BodyContentCommon, ContentDisposition};
use mailparse::{parse_header, MailHeaderMap};
use native_tls::TlsStream;

const DOMAIN: &str = "imap.exmail.qq.com";

pub struct Client {
    imap_session: RefCell<imap::Session<TlsStream<std::net::TcpStream>>>,
}

impl Client {
    pub fn new(username: &str, password: &str) -> Result<Self, imap::Error> {
        let tls = native_tls::TlsConnector::builder().build().unwrap();
        let client = imap::connect((DOMAIN, 993), DOMAIN, &tls)?;

        Ok(Self {
            imap_session: RefCell::new(client.login(username, password).map_err(|e| e.0)?),
        })
    }

    pub fn mail_boxes(&self) -> Result<Vec<MailBox>, imap::Error> {
        let mut mail_boxes = vec![];
        let mut session = self.imap_session.borrow_mut();
        for box_name in session.list(None, Some("*")).unwrap().iter() {
            mail_boxes.push(MailBox {
                client: self,
                name: utf7_imap::decode_utf7_imap(box_name.name().to_string()),
                mail_box: session.select(box_name.name())?,
            })
        }

        Ok(mail_boxes)
    }

    pub fn get(&self, mail_box_name: &str) -> Option<MailBox> {
        let mail_boxes = self.mail_boxes().unwrap();
        for mail_box in mail_boxes {
            if mail_box.name == mail_box_name {
                return Some(mail_box);
            }
        }

        None
    }
}

pub struct MailBox<'c> {
    client: &'c Client,
    name: String,
    mail_box: imap::types::Mailbox,
}

impl<'c> MailBox<'c> {
    pub fn filter(
        &'c self,
        subject_pattern: &str,
        start_datetime: chrono::DateTime<FixedOffset>,
    ) -> MailFilter<'c> {
        MailFilter {
            mail_box: self,
            subject_pattern: subject_pattern.to_string(),
            start_datetime,
            end_datetime: "9999-12-01T00:00:00Z"
                .parse::<chrono::DateTime<FixedOffset>>()
                .unwrap(),
            regex: false,
            reverse: false,
        }
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn download(&self, mail_uid: u32) -> Option<HashMap<String, Vec<u8>>> {
        let mut session = self.client.imap_session.borrow_mut();
        let messages = session.fetch(mail_uid.to_string(), "BODY[]").unwrap();
        let message = messages.iter().next().unwrap();
        let body_parsed = mailparse::parse_mail(message.body().unwrap_or_default()).unwrap();
        let mut mail_data: HashMap<String, Vec<u8>> = HashMap::new();

        for subpart in body_parsed.subparts.iter() {
            if let Some(content_type) = subpart.get_headers().get_first_value("Content-Disposition")
            {
                let filename = content_type
                    .split(';')
                    .nth(1)
                    .unwrap()
                    .trim()
                    .replace('"', "")
                    .replace("filename=", "");

                mail_data.insert(filename, subpart.get_body_raw().unwrap());
            }
        }

        Some(mail_data)
    }
}

pub struct MailFilter<'c> {
    mail_box: &'c MailBox<'c>,
    subject_pattern: String,
    start_datetime: chrono::DateTime<FixedOffset>,
    end_datetime: chrono::DateTime<FixedOffset>,
    regex: bool,
    reverse: bool,
}

impl<'c> MailFilter<'c> {
    pub fn end_date(&mut self, end_datetime: chrono::DateTime<FixedOffset>) -> &mut Self {
        self.end_datetime = end_datetime;
        self
    }

    pub fn regex(&mut self, regex: bool) -> &mut Self {
        self.regex = regex;
        self
    }

    pub fn reverse(&mut self, reserve: bool) -> &mut Self {
        self.reverse = reserve;
        self
    }

    pub fn fetch(&self) -> Vec<Mail> {
        let mut session = self.mail_box.client.imap_session.borrow_mut();
        let query = format!(
            "SINCE {} BEFORE {}",
            self.start_datetime.format("%d-%b-%Y"),
            self.end_datetime.format("%d-%b-%Y")
        );
        let ret = session.search(query);
        let mut mails = vec![];
        let fetch_query =
            "(INTERNALDATE BODY[HEADER.FIELDS (SUBJECT FROM CC TO)] BODY[TEXT] BODYSTRUCTURE)";

        if let Ok(uids) = ret {
            for uid in uids.into_iter() {
                let messages = session.fetch(uid.to_string(), fetch_query).unwrap();
                let message = if let Some(m) = messages.iter().next() {
                    m
                } else {
                    continue;
                };

                let date = message.internal_date().unwrap();
                // imap only can filter by date, so here we need to filter by time
                if date.timestamp() < self.start_datetime.timestamp()
                    || date.timestamp() > self.end_datetime.timestamp()
                {
                    continue;
                }

                let mut attachments = vec![];
                let bodystructure = message.bodystructure().unwrap();
                if let imap_proto::BodyStructure::Multipart {
                    common: _,
                    bodies,
                    extension: _,
                } = bodystructure
                {
                    for body in bodies.iter() {
                        if let imap_proto::BodyStructure::Basic {
                            common:
                                BodyContentCommon {
                                    ty: _,
                                    disposition:
                                        Some(ContentDisposition {
                                            ty: "attachment",
                                            params: Some(params),
                                        }),
                                    language: _,
                                    location: _,
                                },
                            other: _,
                            extension: _,
                        } = body
                        {
                            attachments.push(Attachment::new(
                                params[0].1.to_string(),
                                params.get(1).map(|v| v.1.parse::<u32>().unwrap()),
                            ))
                        }
                    }
                }

                let header = message.header().unwrap();
                let header_parsed = mailparse::parse_mail(header).unwrap();
                let body_parsed =
                    mailparse::parse_mail(message.text().unwrap_or_default()).unwrap();

                let mail = Mail {
                    uid,
                    subject: header_parsed
                        .headers
                        .get_first_header("Subject")
                        .map(|h| h.get_value())
                        .unwrap_or_default(),
                    from: header_parsed
                        .headers
                        .get_first_header("From")
                        .map(|h| h.get_value())
                        .unwrap_or_default(),
                    to: header_parsed
                        .headers
                        .get_first_header("To")
                        .map(|h| h.get_value())
                        .unwrap_or_default()
                        .split(',')
                        .map(|s| s.trim().to_string())
                        .collect(),
                    cc: header_parsed
                        .headers
                        .get_first_header("CC")
                        .map(|h| h.get_value())
                        .unwrap_or_default()
                        .split(',')
                        .map(|s| s.trim().to_string())
                        .collect(),
                    body: body_parsed
                        .subparts
                        .get(0)
                        .map(|subpart| subpart.get_body().unwrap_or_default())
                        .unwrap_or_default(),
                    internal_date: date,
                    attachments,
                };

                if self.regex {
                    if !regex::Regex::new(&self.subject_pattern)
                        .unwrap()
                        .is_match(&mail.subject)
                    {
                        continue;
                    }
                } else if !mail.subject.contains(&self.subject_pattern) {
                    continue;
                }

                mails.push(mail);
            }
        }

        mails.sort_by_key(|v| -v.internal_date.timestamp());
        if self.reverse {
            mails.reverse()
        }

        mails
    }
}

impl Display for MailBox<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "name: {}, flags: {:?}, exists: {}, recent: {}, unseen: {:?}, permanent_flags: {:?},\
             uid_next: {:?}, uid_validity: {:?}",
            self.name,
            self.mail_box.flags,
            self.mail_box.exists,
            self.mail_box.recent,
            self.mail_box.unseen,
            self.mail_box.permanent_flags,
            self.mail_box.uid_next,
            self.mail_box.uid_validity
        )
    }
}

#[derive(Debug)]
pub struct Mail {
    pub subject: String,
    pub from: String,
    pub to: Vec<String>,
    pub cc: Vec<String>,
    pub uid: u32,
    pub body: String,
    pub internal_date: chrono::DateTime<FixedOffset>,
    pub attachments: Vec<Attachment>,
}

#[derive(Debug)]
pub struct Attachment {
    pub name: String,
    pub size: Option<u32>,
}

impl Attachment {
    fn new(name: String, size: Option<u32>) -> Self {
        let name = format!("Subject: {}", name);
        let (parsed, _) = parse_header(name.as_bytes()).unwrap();
        let name = parsed.get_value();

        Self { name, size }
    }
}
