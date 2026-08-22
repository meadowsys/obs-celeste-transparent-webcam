fn main() {
	tokio::runtime::Builder::new_current_thread()
		.enable_all()
		.build_local(tokio::runtime::LocalOptions::default())
		.expect("failed to create async runtime")
		.block_on(async_main())
}

async fn async_main() {}
