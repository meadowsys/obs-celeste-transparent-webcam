use ct_codecs::{ Base64, Encoder as _ };
use futures::{ SinkExt as _, StreamExt as _ };
use serde::{ Deserialize, Deserializer, Serialize };
use serde::de::{ Error as DeError, Visitor };
use sha2::{ Digest as _, Sha256 };
use std::{ fmt, fs, mem };
use std::borrow::Cow;
use std::io::{ self, Read as _, Write as _ };
use std::sync::Arc;
use std::time::Duration;
use tokio::signal::ctrl_c;
use tokio::sync::Semaphore;
use tokio::sync::broadcast::channel;
use tokio::sync::broadcast::error::TryRecvError;
use tokio::task::spawn_local as spawn;
use tokio::time::{ MissedTickBehavior, interval, sleep };
use tokio_tungstenite::tungstenite::Bytes;
use tokio_tungstenite::tungstenite::client::IntoClientRequest as _;
use tokio_tungstenite::tungstenite::http::header::HeaderValue;
use tokio_tungstenite::tungstenite::protocol::{ CloseFrame, Message };
use ulid::Ulid;

// todo figure out if we can modify filter settings from ws, if so we can do gradual opacity
// use new config section for that i think, [[transparency-source]] or something idk

// todo set default for when not ingame
// todo set default for when in the wrong scene (takes priority over not ingame)

fn main() {
	tokio::runtime::Builder::new_current_thread()
		.enable_all()
		.build_local(Default::default())
		.expect("failed to create runtime")
		.block_on(async_main())
}

async fn async_main() {
	let mut config_buf = Vec::new();
	let mut config = Config::read_or_init(&mut config_buf);

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
			"sec-websocket-protocol",
			HeaderValue::from_static("obswebsocket.msgpack")
		);

	let (mut stream, res) = tokio_tungstenite::connect_async(req)
		.await
		.expect("failed to connect to obs");

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
				// EventSubscription::Scenes
				1 << 2
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

	// todo validate the sources config around here before doing anything else
	// fetch obs for sources and filters and check
	for source in &mut config.sources {
		if source.x_start > source.x_end {
			mem::swap(&mut source.x_start, &mut source.x_end);
		}
		if source.y_start > source.y_end {
			mem::swap(&mut source.y_start, &mut source.y_end);
		}
	}

	let (mut stream_send, mut stream_recv) = stream.split();
	let (shutdown_send, mut shutdown_recv) = channel::<()>(1);
	let semaphore_total = 1000;
	let semaphore = Arc::new(Semaphore::new(semaphore_total));

	// just consuming all ws msgs for now since we have not yet a use for them
	// todo
	let _semaphore = Arc::clone(&semaphore);
	spawn(async move {
		let _permit = _semaphore.acquire_owned();

		loop {
			tokio::select! {
				biased;
				msg = stream_recv.next() => {
					// todo dumping msgs for now <loopTeehee>
					if let Some(Ok(Message::Binary(msg))) = msg
						&& let Ok(msg) = rmp_serde::from_slice::<serde_json::Value>(&msg)
					{
						// todo use dashmap to store ids with their reuquest detalis if needed?
						dbg!(msg);
					}
				}
				_ = shutdown_recv.recv() => { break }
			}
		}
	});

	let _semaphore = Arc::clone(&semaphore);
	let mut shutdown_recv = shutdown_send.subscribe();
	spawn(async move {
		let _permit = _semaphore.acquire_owned();
		let mut interval = interval(Duration::from_millis(config.general.position_polling_interval as _));
		interval.set_missed_tick_behavior(MissedTickBehavior::Delay);

		while matches!(shutdown_recv.try_recv(), Err(TryRecvError::Empty)) {
			interval.tick().await;

			// todo handle the error cases and force disable the filter (treat it as if madeline was not in the box)
			// likely requires refactoring the request logic to its own function and other organisation

			// todo fetch localhost api and send updates as needed
			let position = match reqwest::get("http://localhost:32270/cct/madelineScreenPosition").await {
				Ok(position) => { Some(position.json::<CctPosition>().await) }
				// todo add timestamps?
				Err(e) => {
					eprintln!("error fetching from CCT: {e}");
					None
				}
			};

			// holy shit this is ugly pls fix this eventually
			let position = match position {
				Some(Ok(CctPosition { madeline_screen_position })) => { Some(madeline_screen_position) }
				Some(Err(e)) => {
					// todo add timestamps
					// todo better handling of the "out of map" case to not spam console
					eprintln!("error parsing CCT response: {e:?}");
					None
				}
				None => { None }
			};

			for source in &mut config.sources {
				// todo test position against the config

				let mut should_enable = position.as_ref().map(|position| {
					let x = (source.x_start..=source.x_end).contains(&position.x);
					let y = (source.y_start..=source.y_end).contains(&position.y);
					x && y
				}).unwrap_or(false);

				if !source.enable_when_in_bounds {
					should_enable = !should_enable
				}

				if Some(should_enable) == source.enabled { continue }

				let req = ObsSetSourceFilterEnabled {
					source_name: Cow::Borrowed(&source.source),
					filter_name: Cow::Borrowed(&source.filter),
					filter_enabled: should_enable
				}.wrap_in_request_op_code(Ulid::generate());
				let req = rmp_serde::to_vec_named(&req).unwrap();

				if let Err(e) = stream_send.send(Message::Binary(Bytes::from(req))).await {
					eprintln!("error sending request: {e}");
					continue
				}

				source.enabled = Some(should_enable);
			}
			// todo filter request SetSourceFilterEnabled
			// todo filter request SetSourceFilterSettings
		}
	});

	// todo CurrentProgramSceneChanged for tracking scenes
	// do this via spsc

	ctrl_c().await.expect("failed to listen for ctrl+c");
	while semaphore.available_permits() != semaphore_total {
		sleep(Duration::from_millis(500)).await;
	}
}

#[derive(Deserialize)]
struct Config<'h> {
	general: GeneralConfig,
	#[serde(borrow)]
	obs: ObsConfig<'h>,
	#[serde(rename = "source")]
	sources: Vec<SourceConfig>
}

#[derive(Deserialize)]
struct GeneralConfig {
	#[serde(rename = "position-polling-interval")]
	position_polling_interval: usize
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
struct SourceConfig {
	#[serde(default)]
	scene: Option<String>,
	source: String,
	filter: String,
	#[serde(rename = "x-start")]
	x_start: f32,
	#[serde(rename = "y-start")]
	y_start: f32,
	#[serde(rename = "x-end")]
	x_end: f32,
	#[serde(rename = "y-end")]
	y_end: f32,
	#[serde(rename = "enable-when-in-bounds")]
	enable_when_in_bounds: bool,
	#[serde(default, rename = "opacity-enabled--internal-only-do-not-use-in-config-file")]
	enabled: Option<bool>
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

				let default_config = include_bytes!("../default-config.toml");
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

#[derive(Serialize)]
struct ObsRequest<'h, T> {
	#[serde(rename = "requestType")]
	request_type: Cow<'h, str>,
	#[serde(rename = "requestId")]
	request_id: Ulid,
	#[serde(rename = "requestData")]
	request_data: T
}

#[derive(Deserialize)]
struct CctPosition {
	#[serde(rename = "madelineScreenPosition")]
	madeline_screen_position: MadelineScreenPosition
}

struct MadelineScreenPosition {
	x: f32,
	y: f32
}

impl<'de> Deserialize<'de> for MadelineScreenPosition {
	fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
		struct SmolSquishVisitor;

		impl<'de> Visitor<'de> for SmolSquishVisitor {
			type Value = MadelineScreenPosition;

			fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
				f.write_str("\"x, y\" coords")
			}

			fn visit_str<E: DeError>(self, v: &str) -> Result<MadelineScreenPosition, E> {
				let Some((x, y)) = v.split_once(",") else {
					return Err(E::custom(format!("found not coords in \"x, y\" format: {v}")))
				};

				let x = x.trim().parse().map_err(|e| E::custom(format!("error parsing x as a number: {e}")))?;
				let y = y.trim().parse().map_err(|e| E::custom(format!("error parsing y as a number: {e}")))?;

				Ok(MadelineScreenPosition { x, y })
			}
		}

		deserializer.deserialize_str(SmolSquishVisitor)
	}
}

#[derive(Serialize)]
struct ObsSetSourceFilterEnabled<'h> {
	#[serde(rename = "sourceName")]
	source_name: Cow<'h, str>,
	#[serde(rename = "filterName")]
	filter_name: Cow<'h, str>,
	#[serde(rename = "filterEnabled")]
	filter_enabled: bool
}

impl<'h> ObsRequestType for ObsSetSourceFilterEnabled<'h> {
	fn request_type() -> &'static str {
		"SetSourceFilterEnabled"
	}
}

trait WrapInOpCode: Sized {
	fn wrap_in_request_op_code(self, request_id: Ulid) -> OpCode<ObsRequest<'static, Self>>
	where
		Self: ObsRequestType
	{
		let d = ObsRequest {
			request_type: Cow::Borrowed(Self::request_type()),
			request_id,
			request_data: self
		};

		OpCode { op: 6, d }
	}

	// fn wrap_in_request_batch_op_code(self) -> OpCode<Self> {
	// 	OpCode { op: 8, d: self }
	// }
}

impl<T> WrapInOpCode for T {}

trait ObsRequestType {
	fn request_type() -> &'static str;
}
