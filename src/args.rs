use argh::FromArgs;
#[derive(FromArgs, Clone)]
/// A non-interactive concurrent network downloader
pub struct CommandLineArgs {
    /// the URI to download
    #[argh(option, short = 'u')]
    pub url: String,

    /// output file path, optional
    #[argh(option, short = 'o')]
    pub output: Option<String>,

    /// number of concurrent connections, default is 1, max number of connections is 100
    #[argh(option, default = "1", short = 'c')]
    pub connections: u8,

    /// run in the background
    #[argh(switch, short = 'b')]
    pub background: bool,

    /// verbose mode
    #[argh(switch, short = 'v')]
    pub verbose: bool,
}

/*
The following tests verify the command line arguments parsing functionality.

'Test_args_parsing' ensures that the parsing of valid arguments works as expected.
'Test_args_error' ensures that an error is returned when no arguments are passed.
*/
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_args_parsing() {
        let args = CommandLineArgs::from_args(&["test"], &["--url", "http://example.com", "--background"]).unwrap();
        assert_eq!(args.url, "http://example.com");
        assert!(args.background);
    }

    #[test]
    fn test_args_error() {
        let args = CommandLineArgs::from_args(&["test"], &[]);
        assert!(args.is_err(), "Expected an error when no arguments are passed");
    }
}