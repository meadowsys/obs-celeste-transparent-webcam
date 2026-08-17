use ct_codecs::{ Base64, Encoder as _ };
use futures::{ SinkExt as _, StreamExt as _ };
use serde::{ Deserialize, Serialize };
use sha2::{ Digest as _, Sha256 };
use std::fs;
use std::borrow::Cow;
use std::io::{ self, Read as _, Write as _ };
use tokio_tungstenite::tungstenite::Bytes;
use tokio_tungstenite::tungstenite::client::IntoClientRequest as _;
use tokio_tungstenite::tungstenite::http::header::HeaderValue;
use tokio_tungstenite::tungstenite::protocol::{ CloseFrame, Message };

// todo figure out if we can modify filter settings from ws, if so we can do gradual opacity
// use new config section for that i think, [[transparency-source]] or something idk

fn main() {
	tokio::runtime::Builder::new_current_thread()
		.enable_all()
		.build_local(Default::default())
		.expect("failed to create runtime")
		.block_on(async_main())
}

async fn async_main() {
	let mut config_buf = Vec::new();
	let config = Config::read_or_init(&mut config_buf);
	let url = format!(
		"ws://{host}:{port}",
		host = config.obs.host,
		port = config.obs.port
	);
	let mut req = url
		.into_client_request()
		.expect("invalid connection details");
	req
		.headers_mut()
		.insert(
			"Sec-WebSocket-Protocol",
			HeaderValue::from_static("obswebsocket.msgpack")
		);

	let (mut stream, res) = tokio_tungstenite::connect_async(req)
		.await
		.expect("failed to connect to obs");

	// assert_eq!(res.headers().get("sec-websocket-protocol"), "")
	let protocol = res
		.headers()
		.get("sec-websocket-protocol")
		.expect("protocol header not present in obs connect response")
		.to_str()
		.ok();
	assert_eq!(
		protocol,
		Some("obswebsocket.msgpack"),
		"protocol header not expected value in obs connect response"
	);

	let hello = stream
		.next()
		.await
		.expect("obs did not send hello msg")
		.expect("errored in receiving obs hello msg");

	let Message::Binary(hello) = hello else {
		panic!("expected obs hello message to be binary")
	};

	let hello = rmp_serde::from_slice::<OpCode<ObsHello<'_>>>(&hello)
		.expect("failed to deserialise obs hello");

	assert_eq!(hello.op, 0, "invalid opcode (expected 0)");
	assert_eq!(hello.d.rpc_version, 1, "unsupported rpc version (expected 1)");

	let auth_str = hello.d.auth.map(|auth| {
		let Some(pw) = config.obs.password.as_ref() else {
			panic!("obs requires a password but none was provided");
		};

		let mut hasher = Sha256::new();
		hasher.update(pw.as_bytes());
		hasher.update(auth.salt.as_bytes());
		let base64_secret = hasher.finalize();

		let base64_secret = Base64::encode_to_string(&*base64_secret)
			.expect("calculation overflowed, apparently???");

		let mut hasher = Sha256::new();
		hasher.update(base64_secret.as_bytes());
		hasher.update(auth.challenge.as_bytes());
		let auth_str = hasher.finalize();

		// reusing the buffer from before :3c
		let mut buf = base64_secret;
		// SAFETY: Base64 outputs only valid ASCII
		let buf_bytes = unsafe { buf.as_bytes_mut() };

		Base64::encode(buf_bytes, &*auth_str)
			.expect("calculation overflowed, apparently???");

		// input for both times are the same length (output of sha256)
		// so we can assume no length adjustment or anything needed
		buf
	});

	let identify = OpCode {
		op: 1,
		d: ObsIdentify {
			rpc_version: 1,
			authentication: auth_str.as_deref(),
			// todo
			event_subscriptions: const {
				0
			}
		}
	};
	let identify = rmp_serde::to_vec_named(&identify).unwrap();
	stream
		.send(Message::Binary(Bytes::from(identify)))
		.await
		.expect("failed to send identify msg");

	let identified = stream
		.next()
		.await
		.expect("obs did not reply to identify msg")
		.expect("error in receiving obs reply to identify msg");

	let identified = match identified {
		Message::Binary(identified) => { identified }
		Message::Close(Some(CloseFrame { code, reason })) => {
			panic!("connection unexpectedly closed with code {code}: {reason}")
		}
		Message::Close(None) => {
			panic!("connection unexpectedly closed")
		}
		_ => { panic!("expected obs response to identify msg to be binary identified msg") }
	};

	let identified = rmp_serde::from_slice::<OpCode<ObsIdentified>>(&identified)
		.expect("failed to deserialise obs identified");

	assert_eq!(identified.op, 2, "invalid opcode (expected 2)");
	assert_eq!(identified.d.negotiated_rpc_version, 1, "invalid negotiated rpc version (expected 1)");

	// todo SetSourceFilterEnabled
	// todo CurrentProgramSceneChanged for tracking scenes
	// todo event EventSubscription::Scenes (1 << 2)
	// todo filter request SetSourceFilterEnabled
	// todo filter request SetSourceFilterSettings

}

#[derive(Deserialize)]
struct Config<'h> {
	#[serde(borrow)]
	obs: ObsConfig<'h>,
	#[serde(borrow)]
	source: Vec<SourceConfig<'h>>
}

#[derive(Deserialize)]
struct ObsConfig<'h> {
	#[serde(borrow)]
	host: Cow<'h, str>,
	port: u16,
	#[serde(default)]
	password: Option<Cow<'h, str>>
}

#[derive(Deserialize)]
struct SourceConfig<'h> {
	#[serde(borrow, default)]
	scene: Option<Cow<'h, str>>,
	#[serde(borrow)]
	source: Cow<'h, str>,
	#[serde(borrow)]
	filter: Cow<'h, str>,
	x: u16,
	y: u16,
	width: u16,
	height: u16
}

impl<'h> Config<'h> {
	fn read_or_init(buf: &'h mut Vec<u8>) -> Self {
		let path = "obs-celeste-transparent-webcam-config.toml";

		macro_rules! config_err {
			(open $e:ident) => { panic!("errored trying to open config file: {}", $e) };
			(write $e:ident) => { panic!("errored trying to write config file: {}", $e) };
		}

		let file = fs::OpenOptions::new()
			.read(true)
			.open(path);

		match file {
			Ok(mut file) => {
				file.read_to_end(buf).unwrap_or_else(|e| config_err!(open e));

				toml::from_slice(buf).unwrap_or_else(|e| config_err!(open e))
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

				let default_config = include_bytes!("./default-config.toml");
				file.write_all(default_config)
					.unwrap_or_else(|e| config_err!(write e));

				toml::from_slice(default_config).expect("default config is invalid")
			}

			Err(e) => { config_err!(open e) }
		}
	}
}

#[derive(Deserialize, Serialize)]
struct OpCode<T> {
	op: usize,
	d: T
}

#[derive(Deserialize)]
struct ObsHello<'h> {
	#[serde(borrow, rename = "obsStudioVersion")]
	obs_version: Cow<'h, str>,
	#[serde(borrow, rename = "obsWebSocketVersion")]
	obs_ws_version: Cow<'h, str>,
	#[serde(rename = "rpcVersion")]
	rpc_version: usize,
	#[serde(default, rename = "authentication")]
	auth: Option<ObsHelloAuth<'h>>
}

#[derive(Deserialize)]
struct ObsHelloAuth<'h> {
	#[serde(borrow)]
	challenge: Cow<'h, str>,
	#[serde(borrow)]
	salt: Cow<'h, str>
}

#[derive(Serialize)]
struct ObsIdentify<'h> {
	#[serde(rename = "rpcVersion")]
	rpc_version: usize,
	authentication: Option<&'h str>,
	// https://github.com/obsproject/obs-websocket/blob/master/docs/generated/protocol.md#eventsubscription
	// at the time of writing, only goes up to 1 << 19
	#[serde(rename = "eventSubscriptions")]
	event_subscriptions: u32
}

#[derive(Deserialize)]
struct ObsIdentified {
	#[serde(rename = "negotiatedRpcVersion")]
	negotiated_rpc_version: usize
}
