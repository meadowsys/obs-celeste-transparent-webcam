# OBS + Celeste transparent webcam

Little standalone, _fully cross platform and open source_ tool that tracks Madeline's position in game with the help of [CCT] and makes your webcam transparent when she enters the webcam region.

This extends to enabling/disabling any filter, on any source, for any number of sources/filters, based on Madeline's position.

## Features

- [x] Basic functionality: make your webcam, or any other source transparent when Madeline enters behind it
- [x] Activate/deactivate any filter on any source based on Madeline's positioning
- [ ] Act only in specific OBS scenes (filter by active scene)
- [ ] Gradually get more transparent as madeline approaches region
- [ ] Autodetect region to make transparent for a source based on its position in the OBS canvas
- [ ] Ability to set irregular shapes for the region (using an image mask?)
- [x] Do all of the above for unlimited sources
- [ ] lightweight GUI for configuration (instead of being CLI and config file based only, for a nicer user experience)
- [ ] more, probably (i forgor)

## Getting the program

- Download binary for your platform from [releases page] (coming soon™) and place it in a convenient location (preferably in its own folder, so related files can be neatly next to it)
- Continue setting up with the section below

## Setting up the program

- Run the binary once to generate a default config file next to the binary
- Configure your setup according to the instructions in the generated file
- Make sure OBS is running so the binary can connect to it
- Run the binary again
- If you need to reopen OBS or change the config, you need to restart the program (by pressing ctrl+c, then running it again)

## Building the program from source

This section assumes you have basic knowledge on how to use a terminal.

- Install [`git`] and Rust via [`rustup`]
- Clone the repository to wherever you want it to be using `git clone https://github.com/meadowsys/obs-celeste-transparent-webcam`
- Build with `cargo build --release --bin obs-celeste-transparent-webcam`
- Run the binary with `cargo run --release --bin obs-celeste-transparent-webcam`. Alternatively, copy the binary out from `target/release/obs-celeste-transparent-webcam` and delete at least `target` (the intermediate build artifacts) to save storage space
- Continue setting up with [the section above](#setting-up-the-program)

## Notable differences from viddie's ParrotTransparentCam

- Scene is set per source, they don't all have to be the same
- `x-start` and `y-start` here is `x` and `y`, respectively
- `x-end` is `x` + `width`, and `y-end` is `y` + `height` (`x-end` and `y-end` describes coordinates rather than the width of the rectangle region)
- Changing `enable-when-in-bounds` to false will make the program _disable_ a filter when madeline enters the region and _enable_ it when she leaves

## viddie's script works!! why make a new one?

Main reason: I use a Mac and streamer.bot has officially stated in their documentation that they do not officially support anything other than windows. Additionally, it does not seem to be open source, so I can't even try to build it myself.

Secondary reason: more nice to have features! and also a fun little side project to learn about OBS and its APIs.

[CCT]: https://gamebanana.com/mods/358978
[releases page]: https://github.com/meadowsys/obs-celeste-transparent-webcam/releases
[`rustup`]: https://rust-lang.org/tools/install/
