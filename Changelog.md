# Change Log

## Unreleased next version

Breaking changes

* Renamed the `man` command to `help`. ([18])
* The default query type for the `query` command is now `AAAA`. ([#4])

New

* Added a new `lookup` command. ([#10])
* Added new output formats `human` and `table`. ([#20])
* Output a placeholder for unparseable record data rather than erroring
  out. ([#22])

Bug fixes

Other changes

* Increased minimum supported Rust version to 1.78.

[#4]: https://github.com/NLnetLabs/dnsi/pull/4
[#10]: https://github.com/NLnetLabs/dnsi/pull/10
[#18]: https://github.com/NLnetLabs/dnsi/pull/18
[#20]: https://github.com/NLnetLabs/dnsi/pull/20
[#22]: https://github.com/NLnetLabs/dnsi/pull/22


## 0.1.0

Initial release.

