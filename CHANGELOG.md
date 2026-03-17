# Changelog

## [0.1.4](https://github.com/pszymkowiak/enigma/compare/v0.1.3...v0.1.4) (2026-03-17)


### Features

* add 9 production-ready features ([b66bbc2](https://github.com/pszymkowiak/enigma/commit/b66bbc2cdaa221c39110c06684b48c2960374ed8))
* add Azure/GCS provider support to proxy + auto-create key on first run ([0e894bf](https://github.com/pszymkowiak/enigma/commit/0e894bf9b2fcfc640299a8dfb2817457dabd710c))
* add enigma-web — web UI dashboard with Svelte 5 + axum ([21c53a3](https://github.com/pszymkowiak/enigma/commit/21c53a361488fb80fe9e57814d2f61cc22a7aa97))
* add pipeline benchmarks and performance numbers to README ([fa38cc0](https://github.com/pszymkowiak/enigma/commit/fa38cc04e4a6cf64d34f5dc3a926408b1b8a2e21))
* add Raft HA snapshots, dynamic membership, crash recovery ([fcdde01](https://github.com/pszymkowiak/enigma/commit/fcdde01cf001b539e0b0b850c7c4be180fd496cb))
* CI/CD pipeline, source-available license, i18n READMEs ([#1](https://github.com/pszymkowiak/enigma/issues/1)) ([d01a50e](https://github.com/pszymkowiak/enigma/commit/d01a50e25fbdfadc49991e036ff95900f1dad59d))
* **web:** collapsible sidebar with persistent state ([d9fbdf2](https://github.com/pszymkowiak/enigma/commit/d9fbdf2630e1d79c4e3fadab0e97f6d36c8859a0))


### Bug Fixes

* comprehensive security hardening and robustness improvements ([#2](https://github.com/pszymkowiak/enigma/issues/2)) ([05776d0](https://github.com/pszymkowiak/enigma/commit/05776d05d827e4ef84c1f725b5f03f84f6cd3439))
* deep review hardening — credentials, auth, config validation, SQL safety ([#6](https://github.com/pszymkowiak/enigma/issues/6)) ([688a908](https://github.com/pszymkowiak/enigma/commit/688a9083e30bed9fd207b49a2c63fb61b3610607))
* release asset upload skipped — event_name is caller's not workflow_call ([#7](https://github.com/pszymkowiak/enigma/issues/7)) ([a81291b](https://github.com/pszymkowiak/enigma/commit/a81291bea4a2b9d979d5acb911f991bb0444b016))
* replace unwrap with error handling in gRPC snapshot parser ([#10](https://github.com/pszymkowiak/enigma/issues/10)) ([bd0fd9f](https://github.com/pszymkowiak/enigma/commit/bd0fd9f971a167c05cca027d09e4c4c3339d8acd))
* round 1 — performance, dedup removal, security hardening, optimizations ([#9](https://github.com/pszymkowiak/enigma/issues/9)) ([50bc6c1](https://github.com/pszymkowiak/enigma/commit/50bc6c1d23ddc6914c8d9dfa4080ece3d67db7e3))
* switch release-please to simple type for workspace compat ([ccd2ae7](https://github.com/pszymkowiak/enigma/commit/ccd2ae70030a5fde7e40a708cae0fd9da431a1a3))
* update release-please config for workspace version ([5c948eb](https://github.com/pszymkowiak/enigma/commit/5c948eb33fe2ce08e00ef4d87436f45b1a6cccc4))
* use v0.X.Y tag format (drop enigma- prefix) like rtk/vox ([#11](https://github.com/pszymkowiak/enigma/issues/11)) ([4a0c57b](https://github.com/pszymkowiak/enigma/commit/4a0c57b583631599db110d5c40a34f34fd784e1c))
* vendor OpenSSL for aarch64-linux cross-compilation ([#4](https://github.com/pszymkowiak/enigma/issues/4)) ([8b46d41](https://github.com/pszymkowiak/enigma/commit/8b46d415cebc5f20c1e4304d4966d18cb651801f))

## [0.1.3](https://github.com/pszymkowiak/enigma/compare/enigma-v0.1.2...enigma-v0.1.3) (2026-03-17)


### Bug Fixes

* release asset upload skipped — event_name is caller's not workflow_call ([#7](https://github.com/pszymkowiak/enigma/issues/7)) ([a81291b](https://github.com/pszymkowiak/enigma/commit/a81291bea4a2b9d979d5acb911f991bb0444b016))
* replace unwrap with error handling in gRPC snapshot parser ([#10](https://github.com/pszymkowiak/enigma/issues/10)) ([bd0fd9f](https://github.com/pszymkowiak/enigma/commit/bd0fd9f971a167c05cca027d09e4c4c3339d8acd))
* round 1 — performance, dedup removal, security hardening, optimizations ([#9](https://github.com/pszymkowiak/enigma/issues/9)) ([50bc6c1](https://github.com/pszymkowiak/enigma/commit/50bc6c1d23ddc6914c8d9dfa4080ece3d67db7e3))

## [0.1.2](https://github.com/pszymkowiak/enigma/compare/enigma-v0.1.1...enigma-v0.1.2) (2026-03-17)


### Bug Fixes

* deep review hardening — credentials, auth, config validation, SQL safety ([#6](https://github.com/pszymkowiak/enigma/issues/6)) ([688a908](https://github.com/pszymkowiak/enigma/commit/688a9083e30bed9fd207b49a2c63fb61b3610607))
* vendor OpenSSL for aarch64-linux cross-compilation ([#4](https://github.com/pszymkowiak/enigma/issues/4)) ([8b46d41](https://github.com/pszymkowiak/enigma/commit/8b46d415cebc5f20c1e4304d4966d18cb651801f))

## [0.1.1](https://github.com/pszymkowiak/enigma/compare/enigma-v0.1.0...enigma-v0.1.1) (2026-03-17)


### Features

* add 9 production-ready features ([b66bbc2](https://github.com/pszymkowiak/enigma/commit/b66bbc2cdaa221c39110c06684b48c2960374ed8))
* add Azure/GCS provider support to proxy + auto-create key on first run ([0e894bf](https://github.com/pszymkowiak/enigma/commit/0e894bf9b2fcfc640299a8dfb2817457dabd710c))
* add enigma-web — web UI dashboard with Svelte 5 + axum ([21c53a3](https://github.com/pszymkowiak/enigma/commit/21c53a361488fb80fe9e57814d2f61cc22a7aa97))
* add pipeline benchmarks and performance numbers to README ([fa38cc0](https://github.com/pszymkowiak/enigma/commit/fa38cc04e4a6cf64d34f5dc3a926408b1b8a2e21))
* add Raft HA snapshots, dynamic membership, crash recovery ([fcdde01](https://github.com/pszymkowiak/enigma/commit/fcdde01cf001b539e0b0b850c7c4be180fd496cb))
* CI/CD pipeline, source-available license, i18n READMEs ([#1](https://github.com/pszymkowiak/enigma/issues/1)) ([d01a50e](https://github.com/pszymkowiak/enigma/commit/d01a50e25fbdfadc49991e036ff95900f1dad59d))
* **web:** collapsible sidebar with persistent state ([d9fbdf2](https://github.com/pszymkowiak/enigma/commit/d9fbdf2630e1d79c4e3fadab0e97f6d36c8859a0))


### Bug Fixes

* comprehensive security hardening and robustness improvements ([#2](https://github.com/pszymkowiak/enigma/issues/2)) ([05776d0](https://github.com/pszymkowiak/enigma/commit/05776d05d827e4ef84c1f725b5f03f84f6cd3439))
* switch release-please to simple type for workspace compat ([ccd2ae7](https://github.com/pszymkowiak/enigma/commit/ccd2ae70030a5fde7e40a708cae0fd9da431a1a3))
* update release-please config for workspace version ([5c948eb](https://github.com/pszymkowiak/enigma/commit/5c948eb33fe2ce08e00ef4d87436f45b1a6cccc4))
