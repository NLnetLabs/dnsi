# Change Log

## Unreleased next version

Breaking changes

* Renamed the `man` command to `help`. ([18])
* The default query type for the `query` command is now `AAAA`. ([#4])

New

* Added a new `lookup` command. ([#10])
* Added new output formats `friendly` and `table`. The `friendly` format
  is the new default format. ([#20], [#27])
* Output a placeholder for unparseable record data rather than erroring
  out. ([#22])
* Flags can now be set and unset in the `query` command. ([#23])
* The `query` command now also supports TLS. ([#24])
* IP addresses can now be used as the query name of the `query` command.
  They will be translated into the standard reverse pointer names. In this
  case, if no explicit query type is given, `PTR` will be used. ([#25])

Bug fixes

Other changes

* Increased minimum supported Rust version to 1.78.
* Binary packages are now built and distributed via the [NLnetLabs Package
  repository](https://nlnetlabs.nl/packages/).

[#4]: https://github.com/NLnetLabs/dnsi/pull/4
[#10]: https://github.com/NLnetLabs/dnsi/pull/10
[#18]: https://github.com/NLnetLabs/dnsi/pull/18
[#20]: https://github.com/NLnetLabs/dnsi/pull/20
[#22]: https://github.com/NLnetLabs/dnsi/pull/22
[#23]: https://github.com/NLnetLabs/dnsi/pull/23
[#24]: https://github.com/NLnetLabs/dnsi/pull/24
[#25]: https://github.com/NLnetLabs/dnsi/pull/25
[#27]: https://github.com/NLnetLabs/dnsi/pull/27


## 0.1.0

Initial release.

