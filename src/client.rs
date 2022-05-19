#![allow(dead_code)]

use std::{cell::RefCell, fmt::Display};

use chrono::FixedOffset;
use mailparse::MailHeaderMap;
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

    fn mail_boxes(&self) -> Result<Vec<MailBox>, imap::Error> {
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

    fn get(&self, mail_box_name: &str) -> Option<MailBox> {
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
    fn filter(
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
            reserve: false,
        }
    }
}

pub struct MailFilter<'c> {
    mail_box: &'c MailBox<'c>,
    subject_pattern: String,
    start_datetime: chrono::DateTime<FixedOffset>,
    end_datetime: chrono::DateTime<FixedOffset>,
    regex: bool,
    reserve: bool,
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

    pub fn reserve(&mut self, reserve: bool) -> &mut Self {
        self.reserve = reserve;
        self
    }

    fn done(&self) -> Vec<Mail> {
        let mut session = self.mail_box.client.imap_session.borrow_mut();
        let query = format!(
            "SINCE {} BEFORE {}",
            self.start_datetime.format("%d-%b-%Y"),
            self.end_datetime.format("%d-%b-%Y")
        );
        let ret = session.search(query);
        let mut mails = vec![];
        if let Ok(uids) = ret {
            for uid in uids.into_iter() {
                let messages = session
                    .fetch(
                        uid.to_string(),
                        "(INTERNALDATE BODY.PEEK[HEADER.FIELDS (SUBJECT FROM)])",
                    )
                    .unwrap();
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

                let header = message.header().unwrap();
                let parsed = mailparse::parse_mail(header).unwrap();
                let mail = Mail {
                    uid,
                    subject: parsed
                        .headers
                        .get_first_header("Subject")
                        .unwrap()
                        .get_value(),
                    from: parsed.headers.get_first_header("From").unwrap().get_value(),
                    internal_date: date,
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
    pub uid: u32,
    pub internal_date: chrono::DateTime<FixedOffset>,
}
