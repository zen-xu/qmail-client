#[allow(clippy::or_fun_call)]
mod client;

use std::{fmt::Display, str::FromStr};

use chrono::format::{parse, ParseError, Parsed, StrftimeItems};
use chrono::{Datelike, FixedOffset, NaiveDate, TimeZone};
use clap::{Parser, Subcommand};
use owo_colors::OwoColorize;
use serde::Serialize;
use tabled::{object::Columns, Format, Modify, Table, Tabled};

#[derive(Parser, Debug)]
#[clap(author, version)]
struct Cli {
    #[clap(subcommand)]
    command: Commands,
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
}

#[derive(Debug)]
struct DateTime(chrono::DateTime<FixedOffset>);

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

#[derive(Tabled, Serialize)]
struct SearchResult {
    #[tabled(rename = "id")]
    id: u32,
    #[tabled(rename = "Subject")]
    subject: String,
    #[tabled(rename = "From")]
    from: String,
    #[tabled(rename = "To")]
    to: String,
    #[tabled(rename = "CC")]
    cc: String,
    #[tabled(rename = "Date")]
    date: String,
    #[tabled(rename = "Attachments")]
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
    let client = client::Client::new("xzy@ruitiancapital.com", "KiToJph3o6WsvwWZ").unwrap();
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

            if json {
                println!("{}", serde_json::to_string(&mails).unwrap());
            } else {
                println!(
                    "{}",
                    Table::new(mails)
                        .with(
                            Modify::new(Columns::single(1))
                                .with(Format::new(|s| s.green().to_string()))
                        )
                        .with(
                            Modify::new(Columns::single(6))
                                .with(Format::new(|s| s.bright_black().to_string()))
                        )
                );
            }
        }
        Commands::Folders => todo!(),
    }
}
