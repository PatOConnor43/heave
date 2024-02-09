# heave
heave is a tool used to generate [hurl](https://github.com/Orange-OpenSource/hurl) files from OpenAPI specs.

These files can be used for testing or iterating on feature development.

## Testing
This project uses [cargo-insta](https://crates.io/crates/cargo-insta) to create
snapshots of the output to test against. Insta provides a tool that makes
running these tests and reviewing their output easier. To install it run `cargo
install cargo-insta`. Once this is installed, changes can be reviewed with
`cargo insta test --review`.

If you're just trying to run the tests you can run `cargo test`.

## Releasing
This project uses [cargo-dist](https://opensource.axo.dev/cargo-dist/) and
[cargo-release](https://github.com/crate-ci/cargo-release) for the release
process.

The release process looks like this:
- Checkout master
- Create commit that updates RELEASES.md with notes for the new release and
  push commit
- Run `cargo release patch` (or minor or major) and verify the release looks
  correct
- Run the same command with `--execute`
- The GitHub Action should start immediately for the tag

If you are updating cargo-dist you should also run `cargo dist init` to capture
changes to the action.
