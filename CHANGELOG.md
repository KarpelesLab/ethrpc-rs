# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.3.2](https://github.com/KarpelesLab/ethrpc-rs/compare/v0.3.1...v0.3.2) - 2026-09-20

### Added

- [**breaking**] gate the JSON-RPC client behind a new default `rpc` feature

### Fixed

- drop Handler's Send bounds on wasm32

### Other

- raise declared MSRV to 1.89 to match the HTTP stack

## [0.3.1](https://github.com/KarpelesLab/ethrpc-rs/compare/v0.3.0...v0.3.1) - 2026-07-23

### Added

- make crate wasm-compatible; drop direct tokio dependency
