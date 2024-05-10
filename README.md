# dnsi – A tool to investigate the DNS

`dnsi` is a command line tool to investigate various aspects of the
Domain Name System (DNS).

It is currently in a very early state and will expand over the coming
months.

The tool contains a number of commands. Currently, these are:

* `dnis query` sends a query to a name server or the system’s default
  resolver.
* `dnsi man` displays the man page for any command.


## Binary Packages

Getting started with `dnsi` is really easy by installing a binary package
for either Debian and Ubuntu or for Red Hat Enterprise Linux (RHEL) and
compatible systems such as Rocky Linux. 

You can also build `dnsi` from the source code using Cargo, Rust's build
system and package manager. Cargo lets you to run `dnsi` on almost any
operating system and CPU architecture. Refer to the [building](#building)
section to get started.

### Debian

To install the `dnsi` package, you need the 64-bit version of one of these
Debian versions:

-  Debian Bookworm 12
-  Debian Bullseye 11
-  Debian Buster 10

Packages for the `amd64` and  `x86_64` architectures are available for
all listed versions. In addition, we offer `armhf` architecture
packages for Debian/Raspbian Bullseye, and `arm64` for Buster.

First update the `apt` package index: 

``` bash 
sudo apt update
```

Then install packages to allow `apt` to use a repository over HTTPS:

``` bash
sudo apt install \
  ca-certificates \
  curl \
  gnupg \
  lsb-release
```

Add the GPG key from NLnet Labs:

``` bash
curl -fsSL https://packages.nlnetlabs.nl/aptkey.asc | sudo gpg --dearmor -o /usr/share/keyrings/nlnetlabs-archive-keyring.gpg
```

Now, use the following command to set up the *main* repository:

``` bash
echo \
"deb [arch=$(dpkg --print-architecture) signed-by=/usr/share/keyrings/nlnetlabs-archive-keyring.gpg] https://packages.nlnetlabs.nl/linux/debian \
$(lsb_release -cs) main" | sudo tee /etc/apt/sources.list.d/nlnetlabs.list > /dev/null
```

Update the `apt` package index once more:

``` bash
sudo apt update
```

You can now install `dnsi` with:

``` bash
sudo apt install dnsi
```
### Ubuntu

To install a `dnsi` package, you need the 64-bit version of one of these
Ubuntu versions:

- Ubuntu Jammy 22.04 (LTS)
- Ubuntu Focal 20.04 (LTS)
- Ubuntu Bionic 18.04 (LTS)

Packages are available for the `amd64`/`x86_64` architecture only.

First update the `apt` package index: 

``` bash 
sudo apt update
```

Then install packages to allow `apt` to use a repository over HTTPS:

``` bash
sudo apt install \
  ca-certificates \
  curl \
  gnupg \
  lsb-release
```

Add the GPG key from NLnet Labs:

``` bash
curl -fsSL https://packages.nlnetlabs.nl/aptkey.asc | sudo gpg --dearmor -o /usr/share/keyrings/nlnetlabs-archive-keyring.gpg
```

Now, use the following command to set up the *main* repository:

``` bash
echo \
"deb [arch=$(dpkg --print-architecture) signed-by=/usr/share/keyrings/nlnetlabs-archive-keyring.gpg] https://packages.nlnetlabs.nl/linux/ubuntu \
$(lsb_release -cs) main" | sudo tee /etc/apt/sources.list.d/nlnetlabs.list > /dev/null
```

Update the `apt` package index once more:

``` bash
sudo apt update
```

You can now install `dnsi` with:

``` bash
sudo apt install dnsi
```

### RHEL and compatible systems

To install a `dnsi` package, you need Red Hat Enterprise Linux (RHEL) 7,
8 or 9, or compatible operating system such as Rocky Linux. Packages are
available for the `amd64`/`x86_64` architecture only.

First create a file named `/etc/yum.repos.d/nlnetlabs.repo`, enter this
configuration and save it:

``` text
[nlnetlabs]
name=NLnet Labs
baseurl=https://packages.nlnetlabs.nl/linux/centos/$releasever/main/$basearch
enabled=1
```

Add the GPG key from NLnet Labs:

``` bash
sudo rpm --import https://packages.nlnetlabs.nl/aptkey.asc
```

You can now install `dnsi` with:

``` bash
sudo yum install -y dnsi
```

## Building

`dnsi` is written in Rust. The Rust compiler runs on, and compiles to, a
great number of platforms, though not all of them are equally supported. The
official [Rust Platform
Support](https://doc.rust-lang.org/nightly/rustc/platform-support.html) page
provides an overview of the various support levels.

### Installing Rust

While some system distributions include Rust as system packages, `dnsi`
relies on a relatively new version of Rust, currently 1.74 or newer.
We therefore suggest to use the canonical Rust installation via a tool called
`rustup`.

Assuming you already have `curl` installed, you can install `rustup` and Rust
by simply entering:

``` bash
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
```

Alternatively, visit the [Rust website](https://www.rust-lang.org/tools/install) for other installation methods. Also refer to this page for notes
on updating Rust, and configuring the `PATH` environment variable.

### Installing and updating `dnsi`

After successfully installing Rust, installing `dnsi` is as simple as
entering:

``` bash
cargo install dnsi
```

If you want to update to the latest version of `dnsi`, it’s recommended
to update Rust itself as well, using:

``` bash
rustup update
```

Use the `--force` option to overwrite an existing version with the latest
`dnsi` release:

``` bash
cargo install --locked --force dnsi
```

If you want to install a specific version of `dnsi` using Cargo, explicitly
use the ``--version`` option. If needed, use the ``--force`` option to
overwrite an existing version:
        
``` bash
cargo install --locked --force dnsi --version 0.1.0
```
