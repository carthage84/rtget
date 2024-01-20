use argh::FromArgs;

#[derive(FromArgs)]
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
}
