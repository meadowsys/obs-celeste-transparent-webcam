use serde::Deserialize;
use std::fs;
use std::io::{ self, Read as _, Write as _ };

fn main() {
	let config = Config::read_or_init();
}

#[derive(Deserialize)]
struct Config {
	#[serde(rename = "source-name")]
	source_name: String,
	#[serde(rename = "filter-name")]
	filter_name: String,
	x: u16,
	y: u16,
	width: u16,
	height: u16
}

impl Config {
	fn read_or_init() -> Self {
		let path = "obs-celeste-transparent-webcam-config.txt";

		macro_rules! config_err {
			(open $e:ident) => { panic!("errored trying to open config file: {}", $e) };
			(write $e:ident) => { panic!("errored trying to write config file: {}", $e) };
		}

		let file = fs::OpenOptions::new()
			.read(true)
			.open(path);

		match file {
			Ok(mut file) => {
				let mut buf = Vec::new();
				file.read_to_end(&mut buf).unwrap_or_else(|e| config_err!(open e));

				toml::from_slice(&buf).unwrap_or_else(|e| config_err!(open e))
			}

			Err(e) if matches!(e.kind(), io::ErrorKind::NotFound) => {
				let file = fs::OpenOptions::new()
					.create_new(true)
					.write(true)
					.open(path);

				let mut file = match file {
					Ok(file) => { file }
					Err(e) => { config_err!(write e) }
				};

				let default_config = include_bytes!("./default-config.txt");
				file.write_all(default_config)
					.unwrap_or_else(|e| config_err!(write e));

				toml::from_slice(default_config).expect("default config is invalid")
			}

			Err(e) => { config_err!(open e) }
		}
	}
}
