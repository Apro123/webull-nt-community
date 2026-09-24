# nautilus-webull

![license](https://img.shields.io/github/license/nautechsystems/nautilus_trader?color=blue)

An unofficial [NautilusTrader](https://nautilustrader.io) adapter for [Webull](https://webull.com) market data,
maintained in the [webull-nt-community](https://github.com/Apro123/webull-nt-community) community fork.
It is not affiliated with, endorsed by, or supported by Nautech Systems Pty Ltd or the official
NautilusTrader project, and it is not affiliated with Webull. This crate is not published on
crates.io; build it from this repository.

The `nautilus-webull` crate provides access to a subset of the Webull OpenAPI market data endpoints
(historical bars, snapshots, and ticks) for US stocks/ETFs, signed with the venue's
HMAC-SHA1 request signature scheme.

## NautilusTrader

[NautilusTrader](https://nautilustrader.io) is an open-source, production-grade, Rust-native
engine for multi-asset, multi-venue trading systems.

The system spans research, deterministic simulation, and live execution within a single
event-driven architecture, providing research-to-live semantic parity.

## Feature flags

This crate provides feature flags to control source code inclusion during compilation,
depending on the intended use case:

- `extension-module`: Builds as a Python extension module.
- `high-precision` (default): Enables
  [high-precision mode](https://nautilustrader.io/docs/nightly/getting_started/installation/#precision-mode)
  to use 128-bit value types.
- `python`: Enables Python bindings from [PyO3](https://pyo3.rs).
