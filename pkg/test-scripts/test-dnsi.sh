#!/usr/bin/env bash

set -eo pipefail
set -x

case $1 in
  post-install)
    echo -e "\nDNSI VERSION:"
    dnsi --version

  post-upgrade)
    echo -e "\nDNSI VERSION:"
    dnsi --version
esac

