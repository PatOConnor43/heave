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
