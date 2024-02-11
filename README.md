# heave
heave is a tool used to generate [hurl](https://github.com/Orange-OpenSource/hurl) files from OpenAPI specs.

These files can be used for testing or iterating on feature development.

## Motivation
I started on this tool because I was unhappy with the current solutions for
sharing examples of requests. Plain text files are nice to read, easy to
change, and work well under version control. Hurl has an easy interface that
lets you use these files to help you iterate on a feature or run them as tests
and assert on the output.

## Getting Started
Assuming you have your spec and an output directory created, you use `heave` like this:
```
heave generate <spec.yaml> <output>
```
This will create one hurl file per operation per response. Meaning, if you had
an OAS with a single `ListPets` operation that had an HTTP 200 response and an
404 HTTP response defined, you would get 2 files.

These generated files should include:
- The correct HTTP method (GET, POST, etc.)
- A templated path, using hurl variables as path parameters
- Header parameters (if defined)
- Query parameters (if defined)
- Request Body (if defined)
- Asserts based on the response schema

You will most likely need to go through each file and customize some aspects of
the request, but I hope this tool handles a lot of the foundation for you.

#### Customizing the template
This tool does allow you to also customize the template used to generate it.
This tool relies on the [minijinja](https://crates.io/crates/minijinja) crate
for templating. If you'd like to customize the template you can use `heave
template` to print the default template. Put that content into a file with your
changes. Then call `heave` again with your custom template:
```
heave generate <spec.yaml> <output> --template <template>
```
With this functionality you could include additional headers or remove asserts
entirely.

## Contributing
#### Testing
This project uses [cargo-insta](https://crates.io/crates/cargo-insta) to create
snapshots of the output to test against. Insta provides a tool that makes
running these tests and reviewing their output easier. To install it run `cargo
install cargo-insta`. Once this is installed, changes can be reviewed with
`cargo insta test --review`.

If you're just trying to run the tests you can run `cargo test`.

#### Releasing
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
