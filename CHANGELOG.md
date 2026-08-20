# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.3.0](https://github.com/armanarutiunov/doer/releases/tag/v0.3.0) - 2026-08-20

### Other

- publish to crates.io, and keep a homebrew-core formula ready
- name the stack in the readme
- re-read the clock for the day boundary, and cover terminal setup with the guard
- make the rename durable, report an unlistable projects directory, and stop
- report what a dead writer and a failed final flush cost
- derive the drawn regions from the same geometry the scroll maths uses
- leave when the terminal stops sending input
- keep a save failure until that file is written
- stop a discarded project file polluting the one that was kept
- size the date columns to the dates a section actually holds
- clear the screen behind the help overlay
- describe what the app actually does now
- bound the wait for a wedged write, and make the rename durable
- write a project back to the file it was read from
- centre the bottom rows under the list, and scroll a long project name
- ignore a paste outside a text field
- delete a project's file when undo removes the project
- ignore a paste outside a text field
- survive a panic on the save thread
- put the sidebar rename caret after the # prefix
- read the clock once per batch of input
- tidy the save thread's exit arms
- make quitting actually exit
- clear a save failure only when a later write succeeds
- stop rows overflowing the content column on a narrow terminal
- wire the binary together
- render with ratatui
- drop the toast for a skipped entry
- document the port and rewrite the keybindings from the keymap
- keep fields written by a newer version of doer
- keep unreadable todo entries verbatim instead of refusing to save
- add terminal lifecycle, input events and the theme
- add persistence with the on-disk format preserved
- add rust workspace scaffold
- Sidebar with project management ([#1](https://github.com/armanarutiunov/doer/pull/1))
- add install instructions to README
- Readme
- Initial commit
