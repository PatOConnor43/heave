# Version 0.11.0 (2024-07-09)

New Features:
- Added a new flag (--only-new) prevents this tool from overwriting existing files. This is useful when you write your spec, make changes to the generated files, iterate on the spec, and only want to generate files for the changes.

Chores:
- Updated cargo-dist to 0.18.0

# Version 0.10.1 (2024-03-11)

Bug Fixes:
- Fixed a panic when trying to resolve request bodies for anyOf. It was possible for the schemas involved to result in no child request bodies. This happened when an allOf referenced a schema that was `Any` but this could have happened with `AnyOf`, `Not`, or `OneOf` as well. Now the field will be absent from the request body altogether, which matches existing behavior when an unsupported schema type is encountered.

# Version 0.10.0 (2024-03-08)

New features:
- Support for the allOf attribute. This should generate a hurl request body as well as asserts.

# Version 0.9.0 (2024-02-25)

New features:
- Diagnostics
  - Diagnostics allow users to see issues that this application ran into while trying to parse their spec. This may include malformed references, unsupported kinds, and failing to find an application/json media type (among others). Use the new `--show-diagnostics` flag to print them. If they exist bug the flag isn't enabled, a stderr message will appear letting you know that you can re-run the command.

# Version 0.8.1 (2024-02-24)

Bug Fixes:
  - Fixed an issue where hurl files would not be generated if a request body didn't have an application/json media type

# Version 0.8.0 (2024-02-18)

New features:
- Adds support for Media Types that start with "application/json". This was
  required to support specs that use "application/json;charset=utf-8".
- Add the ability to look up a references Schema inside a Response. Previously
  this application assumed the schema would be defined inline with the
  response.

# Version 0.7.0 (2024-02-11)

New features:
- Adds support for json specs

Chores:
- Add tests to make sure the output from json specs matches yaml specs

# Version 0.6.0 (2024-02-11)

🚨🚨 Breaking change🚨🚨:
- File generation is now behind the `generate` subcommand.
- Please use `heave generate <spec> <output>`

New features:
- `template` subcommand
  - `heave template` now prints the default template
- `generate` subcommand
  - takes `--template` option for custom template

Chores:
- Add Getting Started section to README

# Version 0.5.0 (2024-02-10)

New features:
- Asserts are generated for schemas inside arrays now. Previously the only assert that was created was whether or not it was a collection. Now the schema is analyzed and asserts are generated for the first item in that array.
- Optional response fields are now generated with commented out asserts. This felt like a reasonable compromise since the spec doesn't guarantee that those fields will be present. The behavior works like this:
  - If the name of the field is in the objects required attribute, it will generate with a non-commented out assert
  - If the name of the field is not in the objects required attribute, it will not generate with a non-commented out assert
  - If an object is optional, all of it's children asserts will be commented out
  - If generating asserts for the items in an array, all of those will be commented out because the list _could be_ empty

# Version 0.4.0 (2024-02-09)

Bug Fixes:
- Better JSON pretty printing for request bodies.

Chores:
- Added cargo-insta for snapshot testing. This should make it easy to accumulate some specs and verify the outputs come out as expected
- Added GitHub action for CI to automate test running.

# Version 0.3.0 (2024-02-09)

New features:
- Slight improvement of documentation. This will be helpful when I actually write a decent README.
- Fixed panic
  - The request_body generation assumed that Strings, Numbers, Integers, and Booleans would always have a name associated with them. This isn't the case when you have an Array of these types though.

# Version 0.2.0 (2024-02-8)

New features:
- Added support for request bodies. The schema should be recursively generated in the hurl file with empty strings or 0.

# Version 0.1.0 (2024-02-8)

Initial Release! Here are some things that seem to work:
- Successfully parses OpenAPI parameters for every operation
- Path Parameters are included in hurl files as variables
- Query Parameters are available under the [QueryStringParams] section
- Header Params are also included
- Asserts are included based off schema definition for each response
