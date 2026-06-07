// w16-langs\w16-cc\src\cli\tokenizer.rs
//
//! Токенизатор аргументов командной строки.

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Token {
    SubCommand(String),
    ShortFlag(String),
    LongFlag(String),
    FlagValue(String),
    Positional(String),
}

impl Token {
    pub fn as_str(&self) -> &str {
        match self {
            Self::SubCommand(s) | Self::ShortFlag(s) | Self::LongFlag(s)
            | Self::FlagValue(s) | Self::Positional(s) => s,
        }
    }
}

pub struct Tokenizer;

impl Tokenizer {
    pub fn tokenize() -> Vec<Token> {
        let argv: Vec<String> = std::env::args().collect();
        Self::tokenize_argv(&argv)
    }

    pub fn tokenize_argv(argv: &[String]) -> Vec<Token> {
        let args = match argv.get(1..) {
            Some(a) if !a.is_empty() => a,
            _ => return Vec::new(),
        };

        let mut tokens = Vec::with_capacity(args.len());
        let mut iter = args.iter().enumerate();

        while let Some((i, arg)) = iter.next() {
            if i == 0 {
                tokens.push(Token::SubCommand(arg.clone()));
                continue;
            }

            if let Some(flag) = arg.strip_prefix("--") {
                if flag.is_empty() {
                    tokens.push(Token::Positional(arg.clone()));
                    continue;
                }
                tokens.push(Token::LongFlag(arg.clone()));
                if let Some(next) = args.get(i + 1) {
                    if !next.starts_with('-') {
                        tokens.push(Token::FlagValue(next.clone()));
                        iter.next();
                    }
                }
            } else if arg.starts_with('-') && arg.len() > 1 {
                tokens.push(Token::ShortFlag(arg.clone()));
            } else {
                tokens.push(Token::Positional(arg.clone()));
            }
        }

        tokens
    }
}