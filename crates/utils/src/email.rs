use crate::{
  error::{LemmyErrorExt, LemmyErrorType, LemmyResult},
  settings::structs::Settings,
};
use html2text;
use lettre::{
  message::{Mailbox, MultiPart},
  transport::smtp::{authentication::Credentials, extension::ClientId},
  Address,
  AsyncTransport,
  Message,
};
use rosetta_i18n::{Language, LanguageId};
use std::str::FromStr;
use translations::Lang;
use uuid::Uuid;

pub mod translations {
  rosetta_i18n::include_translations!();
}

type AsyncSmtpTransport = lettre::AsyncSmtpTransport<lettre::Tokio1Executor>;

pub async fn send_email(
  subject: &str,
  to_email: &str,
  to_username: &str,
  html: &str,
  settings: &Settings,
) -> LemmyResult<()> {
  let email_config = settings.email.clone().ok_or(LemmyErrorType::NoEmailSetup)?;
  let domain = settings.hostname.clone();

  let (smtp_server, smtp_port) = {
    let email_and_port = email_config.smtp_server.split(':').collect::<Vec<&str>>();
    let email = *email_and_port
      .first()
      .ok_or(LemmyErrorType::EmailRequired)?;
    let port = email_and_port
      .get(1)
      .ok_or(LemmyErrorType::EmailSmtpServerNeedsAPort)?
      .parse::<u16>()?;

    (email, port)
  };

  // use usize::MAX as the line wrap length, since lettre handles the wrapping for us
  let plain_text = html2text::from_read(html.as_bytes(), usize::MAX)?;

  let smtp_from_address = &email_config.smtp_from_address;

  let email = Message::builder()
    .from(
      smtp_from_address
        .parse()
        .with_lemmy_type(LemmyErrorType::InvalidEmailAddress(
          smtp_from_address.into(),
        ))?,
    )
    .to(Mailbox::new(
      Some(to_username.to_string()),
      Address::from_str(to_email)
        .with_lemmy_type(LemmyErrorType::InvalidEmailAddress(to_email.into()))?,
    ))
    .message_id(Some(format!("<{}@{}>", Uuid::new_v4(), settings.hostname)))
    .subject(subject)
    .multipart(MultiPart::alternative_plain_html(
      plain_text,
      html.to_string(),
    ))
    .with_lemmy_type(LemmyErrorType::EmailSendFailed)?;

  // don't worry about 'dangeous'. it's just that leaving it at the default configuration
  // is bad.

  // Set the TLS
  let mut builder = match email_config.tls_type.as_str() {
    "starttls" => AsyncSmtpTransport::starttls_relay(smtp_server)?.port(smtp_port),
    "tls" => AsyncSmtpTransport::relay(smtp_server)?.port(smtp_port),
    _ => AsyncSmtpTransport::builder_dangerous(smtp_server).port(smtp_port),
  };

  // Set the creds if they exist
  let smtp_password = email_config.smtp_password();
  if let (Some(username), Some(password)) = (email_config.smtp_login, smtp_password) {
    builder = builder.credentials(Credentials::new(username, password));
  }

  let mailer = builder.hello_name(ClientId::Domain(domain)).build();

  mailer
    .send(email)
    .await
    .with_lemmy_type(LemmyErrorType::EmailSendFailed)?;

  Ok(())
}

#[allow(clippy::expect_used)]
pub fn lang_str_to_lang(lang: &str) -> Lang {
  let lang_id = LanguageId::new(lang);
  Lang::from_language_id(&lang_id).unwrap_or_else(|| {
    let en = LanguageId::new("en");
    Lang::from_language_id(&en).expect("default language")
  })
}
