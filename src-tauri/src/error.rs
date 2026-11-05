/// Everything a call to Firebase Auth or the server can end with.
#[derive(Debug, Clone)]
pub enum Error {
    /// No session, or it can no longer be refreshed.
    SignedOut,
    /// A Firebase Auth error code, e.g. "EMAIL_EXISTS".
    Firebase(String),
    /// The server answered with this status and its `error` text.
    Status(u16, String),
    /// No answer at all.
    Network,
}

impl Error {
    /// Errors after which the stored session is useless.
    pub fn ends_session(&self) -> bool {
        match self {
            Error::SignedOut => true,
            Error::Firebase(code) => ["TOKEN_EXPIRED", "USER_DISABLED", "USER_NOT_FOUND", "INVALID_REFRESH_TOKEN", "INVALID_GRANT_TYPE"]
                .iter()
                .any(|p| code.starts_with(p)),
            _ => false,
        }
    }

    pub fn status(&self) -> u16 {
        if let Error::Status(code, _) = self { *code } else { 0 }
    }

    /// What the person reads.
    pub fn message(&self) -> String {
        match self {
            Error::SignedOut => tr!("Нужно войти заново"),
            Error::Network => tr!("Нет связи, проверь интернет"),
            Error::Firebase(code) => {
                let code = code.split(' ').next().unwrap_or(code);
                match code {
                    "INVALID_PASSWORD" => tr!("Неверный пароль. Если раньше вход был через Google, нажми «Назад» и «Войти через Google»"),
                    "WEAK_PASSWORD" => tr!("Пароль слишком простой"),
                    "INVALID_EMAIL" | "MISSING_EMAIL" => tr!("Проверь адрес почты"),
                    "MISSING_PASSWORD" => tr!("Введи пароль"),
                    "TOO_MANY_ATTEMPTS_TRY_LATER" => tr!("Слишком много попыток, попробуй чуть позже"),
                    "USER_DISABLED" => tr!("Этот аккаунт отключён"),
                    "BAD_CALLBACK" => tr!("Вход не завершился, попробуй ещё раз"),
                    _ => tr!("Не получилось войти ({code})", code = code),
                }
            }
            Error::Status(status, text) => match (status, text.as_str()) {
                (400, "that is your own invite") => tr!("Это твоя собственная ссылка"),
                (400, "not an invite code") => tr!("Не похоже на ссылку-приглашение"),
                (400, "too many contacts") => tr!("В списке уже 200 человек"),
                (404, _) => tr!("Не нашли"),
                (409, _) => tr!("Уже есть в списке"),
                (429, _) => tr!("Слишком много попыток, попробуй через час"),
                _ => tr!("Сервер не ответил, попробуй ещё раз"),
            },
        }
    }
}

impl From<reqwest::Error> for Error {
    fn from(_: reqwest::Error) -> Self {
        Error::Network
    }
}

pub type Result<T> = std::result::Result<T, Error>;
