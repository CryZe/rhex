# rhex

<p align="center">
  <a href="https://travis-ci.org/dpc/rhex">
      <img src="https://img.shields.io/travis/dpc/rhex/master.svg?style=flat-square" alt="Build Status">
  </a>
  <a href="https://gitter.im/dpc/rhex">
      <img src="https://img.shields.io/badge/GITTER-join%20chat-green.svg?style=flat-square" alt="Gitter Chat">
  </a>
</p>


## Introduction

Simple ASCII terminal hexagonal map  roguelike written in Rust [Rust][rust-home].

~~You can try it out by pointing your **ssh client** to: rhex [at] rhex.dpc.pw (password is obvious).~~ (temporary unavailable) Note: **Make sure your terminal supports 256 colors and exports `TERM=xterm-256color`!**

The core goal of the project:

* ASCI/Unicode pure terminal UI first
* hexagonal map with tactical positioning

It's also intendent to exercise and practice my [Rust][rust-home] knowledge.

Previous iteration of this idea was/is: [Rustyhex][rustyhex] . This two project
might merge into whole at some point.

Rhex is using [hex2d-rs - Hexagonal grid map utillity library][hex2d-rs].

[rust-home]: http://rust-lang.org
[rustyhex]: //github.com/dpc/rustyhex
[hex2d-rs]: //github.com/dpc/hex2d-rs

## Overview

![RustyHex screenshot][ss]

[ss]: http://i.imgur.com/gb2TZlj.png

Watch *rhex* gameplay video:

[![asciicast](https://asciinema.org/a/34224.png)](https://asciinema.org/a/34224)

## Running

Game requires terminal with 256-color support, and basic Unicode font.

	git clone https://github.com/dpc/rhex.git
	cd rhex
	cargo run --release

## Status

The game is playable but not feature and gameplay wise complete.

*rhex* is actively seeking collaborators. If you'd like to practice your Rust
or/and find roguelikes interesting ping `@dpc` on [rhex gitter channel] and we
can get your started.

[Report problems and ideas][issues]

[issues]: https://github.com/dpc/rhex/issues
