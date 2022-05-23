mod client;
mod search;

use std::fs::{self, File};
use std::io::Write;
use std::{fmt::Display, str::FromStr};

use chrono::format::{parse, ParseError, Parsed, StrftimeItems};
use chrono::{Datelike, FixedOffset, NaiveDate, TimeZone};
use clap::{Parser, Subcommand};
use serde::Serialize;
use serde_json::Value;

#[derive(Parser, Debug)]
#[clap(author, version)]
struct Cli {
    #[clap(subcommand)]
    command: Commands,
    #[clap(long)]
    username: Option<String>,
    #[clap(long)]
    password: Option<String>,
}

#[derive(Subcommand, Debug)]
enum Commands {
    Folders,
    Search {
        subject_query: String,

        #[clap(long, default_value_t = {
        let now = chrono::Local::now();
        let start_datetime = NaiveDate::from_ymd(now.year(), now.month(), now.day()).and_hms(0, 0, 0);
        DateTime(now.offset().from_local_datetime(&start_datetime).unwrap())
        })]
        start_datetime: DateTime,

        #[clap(long, default_value_t = {
        let now = chrono::Local::now();
        let start_datetime = NaiveDate::from_ymd(9999, 12, 31).and_hms(0, 0, 0);
        DateTime(now.offset().from_local_datetime(&start_datetime).unwrap())
        })]
        end_datetime: DateTime,

        #[clap(long)]
        regex: bool,
        #[clap(long)]
        reserve: bool,
        #[clap(short, long, default_value_t = String::from("INBOX"))]
        mail_box: String,

        #[clap(long)]
        json: bool,
    },
    Download {
        mail_uid: u32,
    },
}

#[derive(Debug)]
struct DateTime(chrono::DateTime<FixedOffset>);

#[allow(clippy::or_fun_call)]
impl FromStr for DateTime {
    type Err = ParseError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let mut parsed = Parsed::new();

        if parse(&mut parsed, s, StrftimeItems::new("%Y-%m-%dT%H:%M:%S")).is_err() {
            parse(&mut parsed, s, StrftimeItems::new("%Y-%m-%d")).unwrap();
        }

        // set default values
        if parsed.hour_div_12.is_none() {
            parsed.set_hour(0).unwrap();
        }
        parsed.minute = parsed.minute.or(Some(0));
        parsed.second = parsed.second.or(Some(0));
        parsed.nanosecond = parsed.nanosecond.or(Some(0));

        let now = chrono::Local::now();
        parsed.offset = parsed.offset.or(Some(now.offset().local_minus_utc()));

        Ok(DateTime(parsed.to_datetime()?))
    }
}

impl Display for DateTime {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.0.format("%Y-%m-%dT%H:%M:%S").to_string().as_str())
    }
}

#[derive(Serialize)]
struct SearchResult {
    id: u32,
    subject: String,
    from: String,
    to: String,
    cc: String,
    date: String,
    attachments: String,
}

impl SearchResult {
    fn from_mail(mail: client::Mail) -> Self {
        SearchResult {
            id: mail.uid,
            subject: mail.subject,
            from: mail.from,
            to: mail.to.join("\n"),
            cc: mail.cc.join("\n"),
            date: mail.internal_date.to_rfc3339(),
            attachments: mail
                .attachments
                .iter()
                .map(|a| a.name.clone())
                .collect::<Vec<String>>()
                .join("\n"),
        }
    }
}

fn main() {
    let cli = Cli::parse();
    let (username, password) = if cli.username.is_none() || cli.password.is_none() {
        let qmail_passwd = dirs::home_dir().unwrap().join(".qmail_pass");
        let value: Value =
            serde_json::from_str(fs::read_to_string(qmail_passwd).unwrap().as_str()).unwrap();
        (
            value["username"].as_str().unwrap().to_string(),
            value["password"].as_str().unwrap().to_string(),
        )
    } else {
        (cli.username.unwrap(), cli.password.unwrap())
    };

    let client = client::Client::new(&username, &password).unwrap();
    match cli.command {
        Commands::Search {
            subject_query,
            start_datetime,
            end_datetime,
            regex,
            reserve,
            mail_box,
            json,
        } => {
            if json {
                let mail_box = client.get(&mail_box).unwrap();
                let mails = mail_box
                    .filter(&subject_query, start_datetime.0)
                    .end_date(end_datetime.0)
                    .regex(regex)
                    .reverse(reserve)
                    .fetch();
                let mails = mails
                    .into_iter()
                    .map(SearchResult::from_mail)
                    .collect::<Vec<_>>();
                println!("{}", serde_json::to_string(&mails).unwrap());
            } else {
                search::run(
                    client,
                    subject_query,
                    start_datetime.0,
                    end_datetime.0,
                    regex,
                    reserve,
                    mail_box,
                )
                .unwrap();
            }
        }
        Commands::Download { mail_uid } => {
            let mail_box = client.get("INBOX").unwrap();
            let attachments = mail_box.download(mail_uid).unwrap_or_default();
            for (attachment_name, attachment_data) in attachments {
                let mut file = File::create(&attachment_name).unwrap();
                println!("{}", attachment_name);
                file.write_all(&attachment_data[..]).unwrap();
            }
        }
        Commands::Folders => todo!(),
    }
}
