#![allow(dead_code)]

use std::{cell::RefCell, fmt::Display};

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
}

pub struct MailBox<'c> {
    client: &'c Client,
    name: String,
    mail_box: imap::types::Mailbox,
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
