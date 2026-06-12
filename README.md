
> [!WARNING]
> This project is very much still a work in progress. I'm still fleshing out many of the core systems, so much of the code is undocumented. There are a few notable exceptions that I recommend starting with if you're interested in exploring the project:
> - [kernel/src/loader.rs](./kernel/src/loader.rs)
> - [kernel/src/scheduler.rs](./kernel/src/scheduler.rs)

<details>
<summary>Table of Contents</summary>

- [Rhubarb](#rhubarb1)
  - [Quick Start](#quick-start)
  - [Learn More](#learn-more)
  - [License](#license)

</details>

<!-- cargo-rdme start -->

# Rhubarb[^1]

An operating system where executables act as libraries.

See the [design document](./docs/DESIGN.md) for an overview of the system.

## Quick Start

Execute the [runner tool](./tools/runner) to build and run Rhubarb through QEMU. You will need `git`, `cargo`, `rustc`, `ld`, `ar`, and `qemu-system-x86_64` available on the host system.

```sh
git clone https://github.com/rtthw/rhubarb
cd rhubarb/tools/runner
cargo run
```

## Learn More

- [Notes on the project's architecture](./docs/ARCHITECTURE.md)
- [How to contribute](./docs/CONTRIBUTING.md)

## License

[MIT](./LICENSE)



[^1]: Temporary code name. I'll come up with a more permanent one when the project is ready for the first release.
