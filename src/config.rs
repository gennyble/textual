use std::{
    num::ParseIntError,
    path::{Path, PathBuf},
};

use confindent::{Confindent, ParseError};
use getopts::{Fail, Options};
use thiserror::Error;

pub struct Config {
    font_cache_path: PathBuf,
    port: u16,
    scheme: Option<String>,
    host: Option<String>,
}

impl Config {
    pub fn font_cache_path(&self) -> &Path {
        &self.font_cache_path
    }

    pub fn port(&self) -> u16 {
        self.port
    }

    pub fn scheme(&self) -> Option<&str> {
        self.scheme.as_deref()
    }

    pub fn host(&self) -> Option<&str> {
        self.host.as_deref()
    }

    fn usage(opts: &Options) {
        print!("{}", opts.usage("Usage: textual [options]"))
    }

    fn parse_hostname<S: AsRef<str>>(string: S) -> Result<(Option<String>, String), ConfigError> {
        let string = string.as_ref();

        match string.find(':') {
            Some(ind) => {
                let (scheme, host) = string.split_at(ind);
                let host = host
                    .strip_prefix("://")
                    .ok_or(ConfigError::HostnameParseError(string.into()))?;

                if !"https".contains(scheme) {
                    return Err(ConfigError::InvalidScheme(scheme.into()));
                } else {
                    Ok((Some(scheme.into()), host.into()))
                }
            }
            None => {
                // No scheme, just a hostname.
                Ok((None, string.into()))
            }
        }
    }

    pub fn get() -> Result<Option<Self>, ConfigError> {
        let args: Vec<String> = std::env::args().collect();

        let mut opts = Options::new();
        opts.optflag("h", "help", "Print this message and exit");
        opts.optopt(
            "c",
            "config",
            "An alternate config file\nDefaults to /etc/textual/textual.conf",
            "FILE",
        );
        opts.optopt(
			"",
			"font-cache",
			"Font cache location. Overrides the config file.\nConfig key: FontCache\nDefaults to /var/lib/textual/fontcache",
			"PATH"
		);
        opts.optopt(
            "p",
            "port",
            "What part the server should listen on\nConfig key: Port\nDefaults to 30211",
            "PORT",
        );
        opts.optopt(
            "",
            "hostname",
            "Host to force in the meta-tags image link\n\
			Overrides the config file.\n\
            Config key: Hostname\n\
			Default is the host header, or localhost if missing",
            "HOSTNAME",
        );
        let matches = opts.parse(&args[1..])?;

        if matches.opt_present("help") {
            Self::usage(&opts);
            return Ok(None);
        }

        let config_location = matches
            .opt_str("config")
            .unwrap_or("/etc/textual/textual.conf".into());
        let conf = Confindent::from_file(config_location)?;

        let font_cache_path = PathBuf::from(
            matches
                .opt_str("font-cache")
                .or(conf.child_value("FontCache").map(|s| s.into()))
                .unwrap_or("/var/lib/textual/fontcache".into()),
        );

        if !font_cache_path.is_dir() {
            return Err(ConfigError::InvalidFontCache(font_cache_path));
        }

        let port_string = matches
            .opt_str("port")
            .or(conf.child_value("Port").map(|s| s.into()));

        let port = if let Some(string) = port_string {
            string.parse()?
        } else {
            30211
        };

        let hostname_string = matches
            .opt_str("hostname")
            .or(conf.child_value("Hostname").map(|s| s.into()));

        let (scheme, host) = match hostname_string {
            Some(s) => {
                let (scheme, host) = Self::parse_hostname(s)?;
                (scheme, Some(host))
            }
            None => (None, None),
        };

        Ok(Some(Self {
            font_cache_path,
            port,
            scheme,
            host,
        }))
    }
}

#[derive(Debug, Error)]
pub enum ConfigError {
    #[error("{0}")]
    CliParseError(#[from] Fail),
    #[error("failed to parse config file: {0}")]
    ConfigParseError(#[from] ParseError),
    #[error("The provided path for the font cache does not exist: '{0}'")]
    InvalidFontCache(PathBuf),
    #[error("Could not parse the hostname as a uri '{0}'")]
    HostnameParseError(String),
    #[error("Valid schemes are http and https. '{0}' is invalid")]
    InvalidScheme(String),
    #[error("Invalid port specified: '{0}'")]
    InvalidPort(#[from] ParseIntError),
}
