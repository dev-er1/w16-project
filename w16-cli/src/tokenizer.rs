// w16-cli\src\tokenizer.rs
//
//! Токенизатор: сырой `argv` -> типизированный поток токенов.
//!
//! Этот этап **не делает** семантической валидации — он только классифицирует
//! каждую строку как вариант [`Token`]. Этап парсера (`parser.rs`) превращает
//! поток в [`Command`].
//!
//! # Грамматика токенов
//!
//! ```text
//! argv[0]         ->  (имя бинарника, пропускается)
//! argv[1]         ->  SubCommand   например "run"
//! -x              ->  ShortFlag    например "-j"
//! --foo           ->  LongFlag     например "--verbose"
//! --foo <value>   ->  LongFlag + FlagValue  (значение — следующий токен)
//! всё остальное   ->  Positional   например "main.w16h"
//! ```

// ---------------------------------------------------------------------------
// Токен
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Token {
    /// Слово субкоманды (argv[1]).
    SubCommand(String),

    /// Флаг с одним дефисом, например `-j`, `-i`.
    ShortFlag(String),

    /// Флаг с двойным дефисом, например `--verbose`.
    LongFlag(String),

    /// Значение, следующее за флагом вида `--key value`.
    FlagValue(String),

    /// Позиционный аргумент (путь, имя и т.д.).
    Positional(String),
}

impl Token {
    pub fn as_str(&self) -> &str {
        match self {
            Token::SubCommand(s) => s,
            Token::ShortFlag(s) => s,
            Token::LongFlag(s) => s,
            Token::FlagValue(s) => s,
            Token::Positional(s) => s,
        }
    }
}

// ---------------------------------------------------------------------------
// Токенизатор
// ---------------------------------------------------------------------------

pub struct Tokenizer;

impl Tokenizer {
    /// Токенизирует `std::env::args()`.
    pub fn tokenize() -> Vec<Token> {
        let argv: Vec<String> = std::env::args().collect();
        Self::tokenize_argv(&argv)
    }

    /// Токенизирует произвольный срез — удобно для юнит-тестов.
    pub fn tokenize_argv(argv: &[String]) -> Vec<Token> {
        // argv[0] — имя бинарника, пропускаем.
        let args = match argv.get(1..) {
            Some(a) if !a.is_empty() => a,
            _ => return Vec::new(),
        };

        let mut tokens = Vec::with_capacity(args.len());
        let mut iter = args.iter().enumerate();

        while let Some((i, arg)) = iter.next() {
            if i == 0 {
                // Первый настоящий аргумент — всегда субкоманда.
                tokens.push(Token::SubCommand(arg.clone()));
                continue;
            }

            if let Some(flag) = arg.strip_prefix("--") {
                if flag.is_empty() {
                    // Голый `--` завершает разбор флагов; остальное — позиционные.
                    tokens.push(Token::Positional(arg.clone()));
                    continue;
                }
                tokens.push(Token::LongFlag(arg.clone()));

                // Смотрим на следующий аргумент: если он не флаг — это значение текущего флага.
                if let Some(next) = args.get(i + 1)
                    && !next.starts_with('-')
                {
                    tokens.push(Token::FlagValue(next.clone()));
                    iter.next(); // потребляем токен значения
                }
            } else if arg.starts_with('-') {
                // Короткий флаг: `-i`, `-j` и т.д.
                tokens.push(Token::ShortFlag(arg.clone()));
            } else {
                tokens.push(Token::Positional(arg.clone()));
            }
        }

        tokens
    }
}

// ---------------------------------------------------------------------------
// Тесты
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn tok(args: &[&str]) -> Vec<Token> {
        let argv: Vec<String> = std::iter::once("w16")
            .chain(args.iter().copied())
            .map(str::to_owned)
            .collect();
        Tokenizer::tokenize_argv(&argv)
    }

    #[test]
    fn test_run_vm_default() {
        let tokens = tok(&["run", "main.w16h"]);
        assert_eq!(tokens[0], Token::SubCommand("run".into()));
        assert_eq!(tokens[1], Token::Positional("main.w16h".into()));
    }

    #[test]
    fn test_short_flag() {
        let tokens = tok(&["run", "main.w16h", "-j"]);
        assert!(tokens.iter().any(|t| *t == Token::ShortFlag("-j".into())));
    }

    #[test]
    fn test_long_flag_with_value() {
        let tokens = tok(&["run", "main.w16h", "--out", "result"]);
        assert!(tokens.iter().any(|t| *t == Token::LongFlag("--out".into())));
        assert!(
            tokens
                .iter()
                .any(|t| *t == Token::FlagValue("result".into()))
        );
    }

    #[test]
    fn test_empty_argv() {
        let tokens = tok(&[]);
        assert!(tokens.is_empty());
    }
}
