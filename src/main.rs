use clap::{Args, Parser, Subcommand};
use itertools::Itertools;
use minijinja::{context, Environment};
use oas3::spec::{
    MediaType, ObjectOrReference, ObjectSchema, Parameter, ParameterIn, RequestBody, Response,
    SchemaType, SchemaTypeSet, Spec,
};
use std::{
    error::Error,
    path::{Path, PathBuf},
};

/// Program to generate hurl files from openapi schemas
#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Debug, Subcommand)]
enum Commands {
    #[command(about = "Generate hurl files from OpenAPI spec")]
    Generate(GenerateArgs),

    #[command(about = "Print the default template")]
    Template,
}

#[derive(Args, Debug)]
struct GenerateArgs {
    #[arg(
        help = "The path to an OpenAPI spec. This spec must not contain references to other files\n"
    )]
    path: PathBuf,

    #[arg(help = "The directory where generated hurl files will be created\n")]
    output: PathBuf,

    #[arg(long, help = "Prints the default template\n")]
    template: Option<PathBuf>,

    #[arg(long, help = "Prints diagnostics to stdout\n")]
    show_diagnostics: bool,

    #[arg(
        long,
        help = "Only generate new files, do not overwrite existing files\n"
    )]
    only_new: bool,

    #[arg(
        long,
        help = r#"A regex to match against paths in the OpenAPI spec. Only paths that match will be included in the generated files.
        
Examples:
  - `pets` will match any path that contains `pets`
  - `^/pets$` will match only the `/pets` path
  - `\{petId\}` will match any path that contains a parameter named `petId`
  - `/export$` will match any path that ends with `/export`
"#
    )]
    include_paths: Option<String>,

    #[arg(
        long,
        help = r#"A regex to match against status codes in the OpenAPI spec. Only status codes that match will be included in the generated files.

Examples:
  - `200` will match only 200 status codes
  - `200|400` will match 200 and 400 status codes
  - `^2[0-9]{2}$` will match all 2xx status codes
  - `[24]0[04]` will match 200, 204, 400, and 404 status codes
"#
    )]
    include_status_codes: Option<String>,

    #[arg(
        long,
        help = r#"A regex to match against operationIDs in the OpenAPI spec. Only operationIDs that match will be included in the generated files. Specifying this option will filter out any operations that do not have an operationID.

Examples:
  - `Pets` will match any operationID that contains `Pets`
  - `^findPets` will match only operationIDs that start with `findPets`
  - `ById$` will match any operationID that ends with `ById`
  - `updatePet|getPetById` will match any operationID that contains `updatePet` or `getPetById`
"#
    )]
    include_operation_ids: Option<String>,
}

/// The struct used to capture output variables.
///
/// Each field defined in this struct will be available to the template. The template uses the
/// minijinja syntax.
#[derive(Clone, Debug)]
pub struct Output {
    pub expected_status_code: u16,
    pub name: String,
    pub hurl_path: String,
    pub oas_path: String,
    pub oas_operation_id: Option<String>,
    pub method: String,
    pub header_parameters: Vec<String>,
    pub query_parameters: Vec<String>,
    pub asserts: Vec<String>,
    pub request_body_parameter: String,
}

#[derive(Debug)]
pub struct GenerateResult {
    outputs: Vec<Output>,
    diagnostics: Vec<HeaveError>,
}

pub enum InputSpecExtension {
    Json,
    Yaml,
}

const DEFAULT_HURL_TEMPLATE: &str = r#"{{ method }} {{ '{{ baseurl }}' }}{{ path | safe }}
Authorization: Bearer {{ '{{ authorization }}' }}
Prefer: code={{ expected_status_code }}
{% for header in header_parameters %}{{ header }}:
{% endfor %}{% if query_parameters %}
[QueryStringParams]
{% for query in query_parameters %}{{ query }}:
{% endfor %}
{% endif %}{{ request_body_parameter }}
HTTP {{ expected_status_code }}
{% if asserts %}
[Asserts]
{% for assert in asserts %}{{ assert }}
{% endfor %}{% endif %}
"#;

#[derive(Debug, thiserror::Error)]
pub enum HeaveError {
    #[error("Error parsing custom minijinja template")]
    JinjaError {
        #[source]
        source: minijinja::Error,
    },
    #[error(
        r#"
---------------------------
MalformedParameterReference

Message: Parameter references must be start with `#/components/parameters/`.
Path: {path}
Operation: {operation}
Reference: {reference}"#
    )]
    MalformedParameterReference {
        operation: String,
        path: String,
        reference: String,
    },
    #[error(r#"
-----------------
MissingComponents

Message: Missing Components definition from schema. Please define a top-level `components` key in your spec."#)]
    MissingComponents,
    #[error(
        r#"
-------------------------
MissingParameterReference

Message: Failed to find parameter reference.
Path: {}
Operation: {}
Reference: {}"#, .context.path, .context.operation, .reference
    )]
    MissingParameterReference {
        context: DiagnosticContext,
        reference: String,
    },
    #[error(
        r#"
-----------------------------
MalformedRequestBodyReference

Message: RequestBody references must be start with `#/components/requestBodies/`.
Path: {}
Operation: {}
Reference: {}"#, .context.path, .context.operation, .reference
    )]
    MalformedRequestBodyReference {
        context: DiagnosticContext,
        reference: String,
    },
    #[error(
        r#"
---------------------------
MissingRequestBodyReference

Message: Failed to find RequestBody reference.
Path: {}
Operation: {}
Reference: {}"#, .context.path, .context.operation, .reference
    )]
    MissingRequestBodyReference {
        context: DiagnosticContext,
        reference: String,
    },
    // TODO maybe this should be allowed?
    #[error(
        r#"
----------------------------
FailedRequestBodyDereference

Message: RequestBodies defined in `#/components/requestBodies/` must not contain references.
Path: {}
Operation: {}
Reference: {}"#, .context.path, .context.operation, .reference
    )]
    FailedRequestBodyDereference {
        context: DiagnosticContext,
        reference: String,
    },
    #[error(
        r#"
------------------------------------------
MissingApplicationJsonRequestBodyMediaType

Message: Missing application/json MediaType for RequestBody.
Path: {}
Operation: {}"#, .context.path, .context.operation,
    )]
    MissingApplicationJsonRequestBodyMediaType { context: DiagnosticContext },
    #[error(
        r#"
-----------------------------------
MissingSchemaDefinitionForMediaType

Message: Missing Schema definition for MediaType.
Path: {}
Operation: {}"#, .context.path, .context.operation,
    )]
    MissingSchemaDefinitionForMediaType { context: DiagnosticContext },
    #[error(
        r#"
------------------------
MalformedSchemaReference

Message: Schema references must be start with `#/components/schemas/`.
Path: {}
Operation: {}
Reference: {}"#, .context.path, .context.operation, .reference
    )]
    MalformedSchemaReference {
        context: DiagnosticContext,
        reference: String,
    },
    #[error(
        r#"
----------------------
MissingSchemaReference

Message: Failed to find Schema reference.
Path: {}
Operation: {}
Reference: {}"#, .context.path, .context.operation, .reference
    )]
    MissingSchemaReference {
        context: DiagnosticContext,
        reference: String,
    },
    // TODO maybe this should be allowed?
    #[error(
        r#"
-----------------------
FailedSchemaDereference

Message: Schemas defined in `#/components/schemas/` must not contain references.
Path: {}
Operation: {}
Reference: {}"#, .context.path, .context.operation, .reference
    )]
    FailedSchemaDereference {
        context: DiagnosticContext,
        reference: String,
    },
    #[error(
        r#"
-------------------------------------------
UnsupportedSchemaKind

Message: Generation based on schemas using AnyOf, OneOf, Not, or Any are not currently supported.
Path: {}
Operation: {}
Detected Kind: {}
JSON path: {}"#,
.context.path, .context.operation, .kind, .jsonpath
    )]
    UnsupportedSchemaKind {
        context: DiagnosticContext,
        kind: String,
        jsonpath: String,
    },
    #[error(
        r#"
--------------------------
UnsupportedStatusCodeRange

Message: Using ranges for HTTP status codes is currently not supported.
Path: {}
Operation: {}"#,
.context.path, .context.operation,
    )]
    UnsupportedStatusCodeRange { context: DiagnosticContext },
    #[error(
        r#"
------------------------------
MalformedResponseBodyReference

Message: Response references must be start with `#/components/responses/`.
Path: {}
Operation: {}
Reference: {}"#, .context.path, .context.operation, .reference
    )]
    MalformedResponseBodyReference {
        context: DiagnosticContext,
        reference: String,
    },
    #[error(
        r#"
----------------------------
MissingResponseBodyReference

Message: Failed to find Response reference.
Path: {}
Operation: {}
Reference: {}"#, .context.path, .context.operation, .reference
    )]
    MissingResponseBodyReference {
        context: DiagnosticContext,
        reference: String,
    },
    // TODO maybe this should be allowed?
    #[error(
        r#"
-----------------------------
FailedResponseBodyDereference

Message: Schemas defined in `#/components/responses/` must not contain references.
Path: {}
Operation: {}
Reference: {}"#, .context.path, .context.operation, .reference
    )]
    FailedResponseBodyDereference {
        context: DiagnosticContext,
        reference: String,
    },
    #[error(
        r#"
----------------------------
Malformed --include-paths Regex

Message: Failed to parse the provided regex.
Source: {}"#, .source
    )]
    MalformedIncludePathsRegex { source: regex_lite::Error },
    #[error(
        r#"
----------------------------
Malformed --include-status-codes Regex

Message: Failed to parse the provided regex.
Source: {}"#, .source
    )]
    MalformedIncludeStatusCodesRegex { source: regex_lite::Error },
    #[error(
        r#"
----------------------------
Malformed --include-operation-ids Regex

Message: Failed to parse the provided regex.
Source: {}"#, .source
    )]
    MalformedIncludeOperationIDsRegex { source: regex_lite::Error },
    #[error(
        r#"
-----------------------------
Request Body Schema Cycle Detected

Message: A cycle was detected in the request body schema. Generation was halted for that schema.
Path: {}
Operation: {}
jsonpath: {}"#, .context.path, .context.operation, .jsonpath
    )]
    RequestBodySchemaCycleDetected {
        context: DiagnosticContext,
        jsonpath: String,
    },
    #[error(
        r#"
-----------------------------
Response Body Schema Cycle Detected

Message: A cycle was detected in the response body schema. Generation was halted for that schema.
Path: {}
Operation: {}
jsonpath: {}"#, .context.path, .context.operation, .jsonpath
    )]
    ResponseBodySchemaCycleDetected {
        context: DiagnosticContext,
        jsonpath: String,
    },
}

#[derive(Debug, Clone)]
pub struct DiagnosticContext {
    operation: String,
    path: String,
}

fn main() -> Result<(), Box<dyn Error>> {
    let cli = Cli::parse();
    match cli.command {
        Commands::Generate(args) => {
            if args.include_paths.is_some() {
                let include_paths = args.include_paths.as_ref().unwrap();
                let valid = regex_lite::Regex::new(include_paths)
                    .map_err(|e| HeaveError::MalformedIncludePathsRegex { source: e });
                if valid.is_err() {
                    let valid = valid.unwrap_err();
                    println!("{}", valid);
                    return Err(valid.into());
                }
            }
            if args.include_status_codes.is_some() {
                let include_status_codes = args.include_status_codes.as_ref().unwrap();
                let valid = regex_lite::Regex::new(include_status_codes)
                    .map_err(|e| HeaveError::MalformedIncludeStatusCodesRegex { source: e });
                if valid.is_err() {
                    let valid = valid.unwrap_err();
                    println!("{}", valid);
                    return Err(valid.into());
                }
            }

            if args.include_operation_ids.is_some() {
                let include_operation_ids = args.include_operation_ids.as_ref().unwrap();
                let valid = regex_lite::Regex::new(include_operation_ids)
                    .map_err(|e| HeaveError::MalformedIncludeOperationIDsRegex { source: e });
                if valid.is_err() {
                    let valid = valid.unwrap_err();
                    println!("{}", valid);
                    return Err(valid.into());
                }
            }

            let output_directory = args.output;
            let output_directory_metadata = std::fs::metadata(&output_directory)?;
            if !output_directory_metadata.is_dir() {
                return Err("Output must be a directory".into());
            }

            let template = match &args.template {
                Some(t) => {
                    let metadata = std::fs::metadata(t)?;
                    if !metadata.is_file() {
                        return Err("Template must be a file".into());
                    }
                    let template_content = std::fs::read_to_string(t);
                    template_content.unwrap()
                }
                None => DEFAULT_HURL_TEMPLATE.to_string(),
            };

            // This is used as a mechanism to validate that the syntax of the template parses
            // correctly before doing more work. The function that writes the output creates its
            // own minijinja Environment.
            let mut jinja_env = Environment::new();
            jinja_env
                .add_template("output.hurl", &template)
                .map_err(|e| HeaveError::JinjaError { source: e })?;

            let input_path = &args.path;
            let input_metadata = std::fs::metadata(input_path)?;
            if !input_metadata.is_file() {
                return Err("Input spec must be a file".into());
            }
            let input_extension = match &input_path.extension() {
                Some(ext) => match ext.to_str() {
                    Some("json") => Ok(InputSpecExtension::Json),
                    Some("yaml") | Some("yml") => Ok(InputSpecExtension::Yaml),
                    _ => Err("Input spec must be json or yaml file"),
                },
                None => Err("Input spec must be json or yaml file"),
            }?;

            let content = std::fs::read_to_string(input_path)?;
            let spec: Spec = match input_extension {
                InputSpecExtension::Json => {
                    oas3::from_json(&content).expect("Could not deserialize input as json")
                }
                InputSpecExtension::Yaml => {
                    oas3::from_yaml(&content).expect("Could not deserialize input as yaml")
                }
            };

            let result = generate(spec);
            let mut final_outputs = result.outputs;
            if args.include_paths.is_some() {
                let include_paths = args.include_paths.unwrap();
                // Regex was validated at the start of the CLI
                let regex = regex_lite::Regex::new(&include_paths).unwrap();
                final_outputs = filter_include_paths_outputs(regex, final_outputs);
            }

            if args.include_status_codes.is_some() {
                let include_status_codes = args.include_status_codes.unwrap();
                // Regex was validated at the start of the CLI
                let regex = regex_lite::Regex::new(&include_status_codes).unwrap();
                final_outputs = filter_include_status_codes_outputs(regex, final_outputs);
            }

            if args.include_operation_ids.is_some() {
                let include_operation_ids = args.include_operation_ids.unwrap();
                // Regex was validated at the start of the CLI
                let regex = regex_lite::Regex::new(&include_operation_ids).unwrap();
                final_outputs = filter_include_operation_ids_outputs(regex, final_outputs);
            }

            if args.only_new {
                let existing_files: Vec<PathBuf> = std::fs::read_dir(&output_directory)?
                    .filter_map(|entry| {
                        if entry.is_err() {
                            return None;
                        }
                        if entry.as_ref().unwrap().file_type().is_err() {
                            return None;
                        }
                        if !entry.as_ref().unwrap().file_type().unwrap().is_file() {
                            return None;
                        }
                        Some(entry.unwrap().path())
                    })
                    .collect();
                final_outputs = filter_only_new_outputs(&existing_files, final_outputs);
            }

            write_outputs(&final_outputs, &template, &output_directory)?;

            if args.show_diagnostics {
                result.diagnostics.iter().for_each(|d| println!("{}", d));
            } else if !result.diagnostics.is_empty() {
                eprintln!("Diagnostics are available. Re-run your previous command with `--show-diagnostics` to see them.")
            }

            Ok(())
        }
        Commands::Template => {
            println!("{}", DEFAULT_HURL_TEMPLATE);
            Ok(())
        }
    }
}

fn filter_include_operation_ids_outputs(
    regex: regex_lite::Regex,
    outputs: Vec<Output>,
) -> Vec<Output> {
    outputs
        .into_iter()
        .filter(|o| {
            if o.oas_operation_id.is_none() {
                return false;
            }
            regex.is_match(o.oas_operation_id.as_ref().unwrap())
        })
        .collect()
}

fn filter_include_status_codes_outputs(
    regex: regex_lite::Regex,
    outputs: Vec<Output>,
) -> Vec<Output> {
    outputs
        .into_iter()
        .filter(|o| regex.is_match(&o.expected_status_code.to_string()))
        .collect()
}

fn filter_include_paths_outputs(regex: regex_lite::Regex, outputs: Vec<Output>) -> Vec<Output> {
    outputs
        .into_iter()
        .filter(|o| regex.is_match(&o.oas_path))
        .collect()
}

fn filter_only_new_outputs(existing_files: &[PathBuf], outputs: Vec<Output>) -> Vec<Output> {
    outputs
        .into_iter()
        .filter(|o| {
            !existing_files.iter().any(|p| {
                let output_file_name = PathBuf::from(&o.name);
                p.ends_with(&output_file_name)
            })
        })
        .collect()
}

fn write_outputs(
    outputs: &[Output],
    template: &str,
    output_directory: &Path,
) -> Result<(), Box<dyn Error>> {
    let output_directory = output_directory.to_path_buf();
    let mut jinja_env = Environment::new();
    // The content of this template should have already been validated
    jinja_env.add_template("output.hurl", template)?;
    let template = jinja_env.get_template("output.hurl")?;

    for output in outputs.iter() {
        let mut file_path = output_directory.clone();
        file_path.push(&output.name);
        let file = std::fs::File::create(file_path)?;
        template.render_to_write(
            context! {
                name => output.name,
                method => output.method,
                path => output.hurl_path,
                expected_status_code => output.expected_status_code,
                header_parameters => output.header_parameters,
                query_parameters => output.query_parameters,
                asserts => output.asserts,
                request_body_parameter => output.request_body_parameter,
            },
            file,
        )?;
    }
    Ok(())
}

fn generate(spec: Spec) -> GenerateResult {
    let mut outputs: Vec<Output> = vec![];
    let mut diagnostics: Vec<HeaveError> = vec![];
    for (path, method, operation) in spec.operations() {
        let name = operation
            .operation_id
            .clone()
            .unwrap_or_else(|| format!("{}_{}", method, path.replace("/", "_")));
        let mut query_parameters: Vec<String> = vec![];
        let mut header_parameters: Vec<String> = vec![];
        let mut request_body_parameter: Option<String> = None;
        let context = DiagnosticContext {
            path: path.to_string(),
            operation: name.to_string(),
        };
        for parameter in operation.parameters.iter() {
            match parameter {
                ObjectOrReference::Ref { ref_path, .. } => {
                    let parameter_name = ref_path.split("#/components/parameters/").nth(1);
                    if parameter_name.is_none() {
                        diagnostics.push(HeaveError::MalformedParameterReference {
                            operation: name.to_string(),
                            path: path.to_string(),
                            reference: ref_path.to_string(),
                        });
                        continue;
                    }
                    let parameter_name = parameter_name.unwrap();
                    let components = &spec.components;
                    if components.is_none() {
                        diagnostics.push(HeaveError::MissingComponents);
                        continue;
                    }
                    let found_parameter =
                        components.as_ref().unwrap().parameters.get(parameter_name);
                    if found_parameter.is_none() {
                        diagnostics.push(HeaveError::MissingParameterReference {
                            context: context.clone(),
                            reference: ref_path.to_string(),
                        });
                        continue;
                    }
                    let found_parameter = found_parameter.unwrap();
                    match found_parameter {
                        ObjectOrReference::Object(param) => {
                            classify_parameter(param, &mut query_parameters, &mut header_parameters);
                        }
                        // TODO add support for nested reference parameters
                        ObjectOrReference::Ref { .. } => {
                            continue;
                        }
                    }
                }
                ObjectOrReference::Object(param) => {
                    classify_parameter(param, &mut query_parameters, &mut header_parameters);
                }
            }
        }

        while let Some(request_body) = &operation.request_body {
            let (request_body, mut inner_diagnostics) =
                resolve_request_body(&spec, request_body, &context);
            diagnostics.append(&mut inner_diagnostics);
            if request_body.is_none() {
                break;
            }
            let request_body = request_body.unwrap();
            let mut media_type: Option<&MediaType> = None;
            for (media_type_key, media_type_val) in request_body.content.iter() {
                if media_type_key.starts_with("application/json") {
                    media_type = Some(media_type_val);
                    break;
                }
            }
            if media_type.is_none() {
                diagnostics.push(HeaveError::MissingApplicationJsonRequestBodyMediaType {
                    context: context.clone(),
                });
                break;
            }
            let media_type = media_type.unwrap();
            let schema = &media_type.schema;
            if schema.is_none() {
                diagnostics.push(HeaveError::MissingSchemaDefinitionForMediaType {
                    context: context.clone(),
                });
                break;
            }
            let schema = schema.as_ref().unwrap();
            let (schema, mut inner_diagnostics) = resolve_schema(&spec, schema, &context);
            diagnostics.append(&mut inner_diagnostics);
            if schema.is_none() {
                break;
            }
            let schema = schema.unwrap();
            let request_body_parameter_tuple =
                generate_request_body_from_schema(&spec, schema, None, &context, "$");
            request_body_parameter = request_body_parameter_tuple.0;
            let mut inner_diagnostics = request_body_parameter_tuple.1;
            diagnostics.append(&mut inner_diagnostics);
            if let Some(body) = &request_body_parameter {
                let a = serde_json::from_str::<serde_json::Value>(body).unwrap();
                let body = serde_json::to_string_pretty(&a);
                if let Ok(body) = body {
                    request_body_parameter = Some(body);
                }
            };
            break;
        }

        if let Some(responses) = &operation.responses {
            for (status_code_str, response) in responses.iter() {
                let mut asserts: Vec<String> = vec![];
                // Check if this is a range like "2XX"
                let parsed_code: Option<u16> = status_code_str.parse().ok();
                if parsed_code.is_none() {
                    diagnostics.push(HeaveError::UnsupportedStatusCodeRange {
                        context: context.clone(),
                    });
                    continue;
                }
                let code = parsed_code.unwrap();
                let name = format!("{}_{}.hurl", name, code);
                let (response, mut inner_diagnostics) =
                    resolve_response(&spec, response, &context);
                diagnostics.append(&mut inner_diagnostics);
                if response.is_none() {
                    continue;
                }
                let response = response.unwrap();
                let mut media_type: Option<&MediaType> = None;
                for (media_type_key, media_type_val) in response.content.iter() {
                    if media_type_key.starts_with("application/json") {
                        media_type = Some(media_type_val);
                        break;
                    }
                }
                if media_type.is_none() {
                    let output = Output {
                        expected_status_code: code,
                        name,
                        hurl_path: path.to_string().replace("{", "{{").replace("}", "}}"),
                        oas_path: path.to_string(),
                        oas_operation_id: operation.operation_id.clone(),
                        method: method.to_string().to_uppercase(),
                        header_parameters: header_parameters.clone(),
                        query_parameters: query_parameters.clone(),
                        asserts: vec![],
                        request_body_parameter: request_body_parameter
                            .clone()
                            .unwrap_or("".to_string()),
                    };
                    outputs.push(output);
                    continue;
                }
                let schema = media_type.unwrap().schema.as_ref();
                if schema.is_none() {
                    diagnostics.push(HeaveError::MissingSchemaDefinitionForMediaType {
                        context: context.clone(),
                    });
                    continue;
                }
                let schema = schema.unwrap();
                let (schema, mut inner_diagnostics) =
                    resolve_schema(&spec, schema, &context);
                diagnostics.append(&mut inner_diagnostics);
                if schema.is_none() {
                    continue;
                }
                let schema = schema.unwrap();
                let is_required = true;
                let (mut new_asserts, mut new_diagnostics) =
                    generate_assert_from_schema(&spec, schema, "$", is_required, &context);
                asserts.append(&mut new_asserts);
                diagnostics.append(&mut new_diagnostics);

                // It's possible for identical asserts to be generated when dealing with
                // polymorphic attributes (like allOf). This cleans that up.
                let asserts: Vec<_> = asserts.into_iter().unique().collect();

                let output = Output {
                    expected_status_code: code,
                    name,
                    hurl_path: path.to_string().replace("{", "{{").replace("}", "}}"),
                    oas_path: path.to_string(),
                    oas_operation_id: operation.operation_id.clone(),
                    method: method.to_string().to_uppercase(),
                    header_parameters: header_parameters.clone(),
                    query_parameters: query_parameters.clone(),
                    asserts: asserts.clone(),
                    request_body_parameter: request_body_parameter
                        .clone()
                        .unwrap_or("".to_string()),
                };
                outputs.push(output)
            }
        }
    }

    GenerateResult {
        outputs,
        diagnostics,
    }
}

fn classify_parameter(
    param: &Parameter,
    query_parameters: &mut Vec<String>,
    header_parameters: &mut Vec<String>,
) {
    match param.location {
        ParameterIn::Query => {
            query_parameters.push(param.name.to_string());
        }
        ParameterIn::Header => {
            header_parameters.push(param.name.to_string());
        }
        _ => {}
    }
}

fn generate_assert_from_schema(
    spec: &Spec,
    schema: &ObjectSchema,
    jsonpath: &str,
    is_required: bool,
    diagnostic_context: &DiagnosticContext,
) -> (Vec<String>, Vec<HeaveError>) {
    // We don't need to generate an assert for a field that is write only
    if schema.write_only.unwrap_or(false) {
        return (vec![], vec![]);
    }

    // Cycle Detection
    let mut parts = jsonpath.split('.').rev().peekable();
    while parts.peek().is_some() {
        let part = parts.next().unwrap();
        if part == "$" {
            break;
        }
        // Check if the immediate next part is the same as the current part. If it is, we have a
        // cycle
        if parts.peek().map_or(false, |next| *next == part) {
            return (
                vec![],
                vec![HeaveError::ResponseBodySchemaCycleDetected {
                    context: diagnostic_context.clone(),
                    jsonpath: jsonpath.to_string(),
                }],
            );
        }
        // Check the next part
        let mut peek_again = parts.clone();
        let _ = peek_again.next();
        if peek_again.next().map_or(false, |next| next == part) {
            return (
                vec![],
                vec![HeaveError::ResponseBodySchemaCycleDetected {
                    context: diagnostic_context.clone(),
                    jsonpath: jsonpath.to_string(),
                }],
            );
        }
    }

    let mut asserts = vec![];
    let mut diagnostics = vec![];
    let is_required_formatter = |jsonpath: &str, default: &str, is_required: bool| -> String {
        format!(
            "{}jsonpath \"{}\" {}",
            if is_required { "" } else { "#" },
            jsonpath,
            default
        )
    };

    // Check composition keywords first
    if !schema.one_of.is_empty() {
        diagnostics.push(HeaveError::UnsupportedSchemaKind {
            context: diagnostic_context.clone(),
            kind: "OneOf".to_string(),
            jsonpath: jsonpath.to_string(),
        });
        return (asserts, diagnostics);
    }
    if !schema.all_of.is_empty() {
        for all_of_schema_or_ref in &schema.all_of {
            let (all_of_schema, mut inner_diagnostics) =
                resolve_schema(spec, all_of_schema_or_ref, diagnostic_context);
            diagnostics.append(&mut inner_diagnostics);

            if let Some(s) = all_of_schema {
                let (mut child_asserts, mut child_diagnostics) = generate_assert_from_schema(
                    spec,
                    s,
                    jsonpath,
                    is_required,
                    diagnostic_context,
                );
                asserts.append(&mut child_asserts);
                diagnostics.append(&mut child_diagnostics);
            }
        }
        return (asserts, diagnostics);
    }
    if !schema.any_of.is_empty() {
        diagnostics.push(HeaveError::UnsupportedSchemaKind {
            context: diagnostic_context.clone(),
            kind: "AnyOf".to_string(),
            jsonpath: jsonpath.to_string(),
        });
        return (asserts, diagnostics);
    }

    // Determine the primary type from schema_type
    let primary_type = match &schema.schema_type {
        Some(type_set) => get_primary_type(type_set),
        None => {
            diagnostics.push(HeaveError::UnsupportedSchemaKind {
                context: diagnostic_context.clone(),
                kind: "Any".to_string(),
                jsonpath: jsonpath.to_string(),
            });
            return (asserts, diagnostics);
        }
    };

    match primary_type {
        Some(SchemaType::Boolean) => {
            asserts.push(is_required_formatter(jsonpath, "isBoolean", is_required))
        }
        Some(SchemaType::String) => {
            asserts.push(is_required_formatter(jsonpath, "isString", is_required))
        }
        Some(SchemaType::Number) => {
            asserts.push(is_required_formatter(jsonpath, "isNumber", is_required))
        }
        Some(SchemaType::Integer) => {
            asserts.push(is_required_formatter(jsonpath, "isInteger", is_required))
        }
        Some(SchemaType::Array) => {
            asserts.push(is_required_formatter(jsonpath, "isCollection", is_required));
            let items = &schema.items;
            if items.is_none() {
                return (asserts, diagnostics);
            }
            let items = items.as_ref().unwrap();
            let inner = resolve_schema_from_schema(spec, items, diagnostic_context);
            match inner {
                (Some(inner), mut inner_diagnostics) => {
                    diagnostics.append(&mut inner_diagnostics);
                    // Take the existing path and index the first element in the list.
                    let inner_jsonpath = format!("{}[0]", jsonpath);

                    // is_required is always false because a list may always be empty
                    let is_required = false;

                    let (mut child_asserts, mut child_diagnostics) = generate_assert_from_schema(
                        spec,
                        inner,
                        inner_jsonpath.as_ref(),
                        is_required,
                        diagnostic_context,
                    );
                    asserts.append(&mut child_asserts);
                    diagnostics.append(&mut child_diagnostics);
                }
                (None, mut inner_diagnostics) => {
                    diagnostics.append(&mut inner_diagnostics);
                    return (asserts, diagnostics);
                }
            }
        }
        Some(SchemaType::Object) => {
            asserts.push(is_required_formatter(jsonpath, "isCollection", is_required));
            let properties = &schema.properties;
            for (name, prop) in properties.iter() {
                let (inner, mut inner_diagnostics) =
                    resolve_schema(spec, prop, diagnostic_context);
                diagnostics.append(&mut inner_diagnostics);
                if inner.is_none() {
                    break;
                }
                let inner = inner.unwrap();

                // There are characters that aren't allowed in jsonpath so we change the format
                // if they're present.
                let inner_jsonpath = if name.chars().any(|c| c == '@' || c == '$') {
                    format!("{}['{}']", jsonpath, name)
                } else {
                    format!("{}.{}", jsonpath, name)
                };
                let child_is_required = is_required && schema.required.contains(name);
                let (mut child_asserts, mut child_diagnostics) =
                    generate_assert_from_schema(
                        spec,
                        inner,
                        inner_jsonpath.as_ref(),
                        child_is_required,
                        diagnostic_context,
                    );
                asserts.append(&mut child_asserts);
                diagnostics.append(&mut child_diagnostics);
            }
        }
        _ => {
            diagnostics.push(HeaveError::UnsupportedSchemaKind {
                context: diagnostic_context.clone(),
                kind: "Any".to_string(),
                jsonpath: jsonpath.to_string(),
            });
        }
    }
    (asserts, diagnostics)
}

/// Extract the primary (non-null) type from a SchemaTypeSet.
fn get_primary_type(type_set: &SchemaTypeSet) -> Option<SchemaType> {
    match type_set {
        SchemaTypeSet::Single(t) => Some(*t),
        SchemaTypeSet::Multiple(types) => {
            // Return the first non-null type
            types.iter().find(|t| **t != SchemaType::Null).copied()
        }
    }
}

/// Resolve a Schema enum (which may be Boolean or Object) into an ObjectSchema reference.
/// When the schema contains a `$ref`, resolves it against the spec's components.
fn resolve_schema_from_schema<'a>(
    spec: &'a Spec,
    schema: &'a oas3::spec::Schema,
    diagnostic_context: &DiagnosticContext,
) -> (Option<&'a ObjectSchema>, Vec<HeaveError>) {
    match schema {
        oas3::spec::Schema::Boolean(_) => (None, vec![]),
        oas3::spec::Schema::Object(obj_or_ref) => {
            resolve_schema(spec, obj_or_ref, diagnostic_context)
        }
    }
}

fn resolve_schema<'a>(
    spec: &'a Spec,
    schema: &'a ObjectOrReference<ObjectSchema>,
    diagnostic_context: &DiagnosticContext,
) -> (Option<&'a ObjectSchema>, Vec<HeaveError>) {
    let mut diagnostics: Vec<HeaveError> = vec![];
    match schema {
        ObjectOrReference::Object(item) => (Some(item), diagnostics),
        ObjectOrReference::Ref { ref_path, .. } => {
            let schema_name = ref_path.split("#/components/schemas/").nth(1);
            if schema_name.is_none() {
                diagnostics.push(HeaveError::MalformedSchemaReference {
                    context: diagnostic_context.clone(),
                    reference: ref_path.to_string(),
                });
                return (None, diagnostics);
            }
            let schema_name = schema_name.unwrap();
            let components = &spec.components;
            if components.is_none() {
                diagnostics.push(HeaveError::MissingComponents);
                return (None, diagnostics);
            }
            let found_schema = components.as_ref().unwrap().schemas.get(schema_name);
            if found_schema.is_none() {
                diagnostics.push(HeaveError::MissingSchemaReference {
                    context: diagnostic_context.clone(),
                    reference: ref_path.to_string(),
                });
                return (None, diagnostics);
            }
            let found_schema = found_schema.unwrap();
            match found_schema {
                ObjectOrReference::Object(schema) => (Some(schema), diagnostics),
                ObjectOrReference::Ref { .. } => {
                    diagnostics.push(HeaveError::FailedSchemaDereference {
                        context: diagnostic_context.clone(),
                        reference: ref_path.to_string(),
                    });
                    (None, diagnostics)
                }
            }
        }
    }
}

fn resolve_request_body<'a>(
    spec: &'a Spec,
    request_body: &'a ObjectOrReference<RequestBody>,
    diagnostic_context: &DiagnosticContext,
) -> (Option<&'a RequestBody>, Vec<HeaveError>) {
    let mut diagnostics: Vec<HeaveError> = vec![];
    match request_body {
        ObjectOrReference::Object(item) => (Some(item), diagnostics),
        ObjectOrReference::Ref { ref_path, .. } => {
            let request_body_name = ref_path.split("#/components/requestBodies/").nth(1);
            if request_body_name.is_none() {
                diagnostics.push(HeaveError::MalformedRequestBodyReference {
                    context: diagnostic_context.clone(),
                    reference: ref_path.to_string(),
                });
                return (None, diagnostics);
            }
            let request_body_name = request_body_name.unwrap();
            let components = &spec.components;
            if components.is_none() {
                diagnostics.push(HeaveError::MissingComponents);
                return (None, diagnostics);
            }
            let found_request_body = components
                .as_ref()
                .unwrap()
                .request_bodies
                .get(request_body_name);
            if found_request_body.is_none() {
                diagnostics.push(HeaveError::MissingRequestBodyReference {
                    context: diagnostic_context.clone(),
                    reference: ref_path.to_string(),
                });
                return (None, diagnostics);
            }
            let found_request_body = found_request_body.unwrap();
            match found_request_body {
                ObjectOrReference::Object(rb) => (Some(rb), diagnostics),
                ObjectOrReference::Ref { .. } => {
                    diagnostics.push(HeaveError::FailedRequestBodyDereference {
                        context: diagnostic_context.clone(),
                        reference: ref_path.to_string(),
                    });
                    (None, diagnostics)
                }
            }
        }
    }
}

fn generate_request_body_from_schema(
    spec: &Spec,
    schema: &ObjectSchema,
    name: Option<String>,
    diagnostic_context: &DiagnosticContext,
    jsonpath: &str,
) -> (Option<String>, Vec<HeaveError>) {
    // We don't need to include this in the request body if it's read only
    if schema.read_only.unwrap_or(false) {
        return (None, vec![]);
    }
    // Cycle Detection
    let mut parts = jsonpath.split('.').rev().peekable();
    while parts.peek().is_some() {
        let part = parts.next().unwrap();
        if part == "$" {
            break;
        }
        // Check if the immediate next part is the same as the current part. If it is, we have a
        // cycle
        if parts.peek().map_or(false, |next| *next == part) {
            return (
                None,
                vec![HeaveError::RequestBodySchemaCycleDetected {
                    context: diagnostic_context.clone(),
                    jsonpath: jsonpath.to_string(),
                }],
            );
        }
        // Check the next part
        let mut peek_again = parts.clone();
        let _ = peek_again.next();
        if peek_again.next().map_or(false, |next| next == part) {
            return (
                None,
                vec![HeaveError::RequestBodySchemaCycleDetected {
                    context: diagnostic_context.clone(),
                    jsonpath: jsonpath.to_string(),
                }],
            );
        }
    }

    let mut diagnostics = vec![];

    // Check composition keywords first
    if !schema.one_of.is_empty() {
        diagnostics.push(HeaveError::UnsupportedSchemaKind {
            context: diagnostic_context.clone(),
            kind: "OneOf".to_string(),
            jsonpath: name.unwrap_or("".to_string()),
        });
        return (None, diagnostics);
    }
    if !schema.all_of.is_empty() {
        let mut child_request_bodies = vec![];
        let mut flattened_object_fields = serde_json::Value::Object(serde_json::Map::new());
        for all_of_schema_or_ref in &schema.all_of {
            let (all_of_schema, mut inner_diagnostics) =
                resolve_schema(spec, all_of_schema_or_ref, diagnostic_context);
            diagnostics.append(&mut inner_diagnostics);
            if let Some(s) = all_of_schema {
                let (request_body, mut inner_diagnostics) = generate_request_body_from_schema(
                    spec,
                    s,
                    None,
                    diagnostic_context,
                    jsonpath,
                );
                diagnostics.append(&mut inner_diagnostics);

                if let Some(body) = &request_body {
                    // In the case of `allOf`, objects need special handling. We create an
                    // empty JSON value and then flatten all the fields on to that single
                    // value. Any primitive fields can just be added directly to
                    // `child_request_bodies`.
                    let mut j = serde_json::from_str::<serde_json::Value>(body).unwrap();
                    if j.is_object() {
                        let inner_map = flattened_object_fields.as_object_mut().unwrap();
                        inner_map.append(j.as_object_mut().unwrap());
                    } else {
                        child_request_bodies.push(request_body);
                    }
                }
            }
        }
        // Only include `flattened_object_fields` if we actually added anything to it.
        if !flattened_object_fields.as_object().unwrap().is_empty() {
            child_request_bodies.push(Some(flattened_object_fields.to_string()));
        }

        // If child_request_bodies is empty we need to communicate that we couldn't build
        // anything.
        if child_request_bodies.is_empty() {
            return (None, diagnostics);
        }

        let stringified_body = child_request_bodies
            .into_iter()
            .flatten()
            .collect::<Vec<String>>()
            .join(",\n");

        return match name {
            Some(name) => (
                Some(format!("\"{}\": {}", name, stringified_body)),
                diagnostics,
            ),
            None => (Some(stringified_body), diagnostics),
        };
    }
    if !schema.any_of.is_empty() {
        diagnostics.push(HeaveError::UnsupportedSchemaKind {
            context: diagnostic_context.clone(),
            kind: "AnyOf".to_string(),
            jsonpath: name.unwrap_or("".to_string()),
        });
        return (None, diagnostics);
    }

    // A small helper that takes properties that may or may not have names and formats them
    // accordingly.
    let single_property_formatter = |name: Option<String>, default: &str| -> String {
        match name {
            Some(name) => format!("\"{}\": {}", name, default),
            None => default.to_string(),
        }
    };

    // Determine the primary type from schema_type
    let primary_type = match &schema.schema_type {
        Some(type_set) => get_primary_type(type_set),
        None => {
            diagnostics.push(HeaveError::UnsupportedSchemaKind {
                context: diagnostic_context.clone(),
                kind: "Any".to_string(),
                jsonpath: name.unwrap_or("".to_string()),
            });
            return (None, diagnostics);
        }
    };

    match primary_type {
        Some(SchemaType::Boolean) => {
            return (Some(single_property_formatter(name, "false")), diagnostics);
        }
        Some(SchemaType::String) => {
            return (Some(single_property_formatter(name, "\"\"")), diagnostics);
        }
        Some(SchemaType::Number) | Some(SchemaType::Integer) => {
            return (Some(single_property_formatter(name, "0")), diagnostics);
        }
        Some(SchemaType::Object) => {
            let properties = &schema.properties;
            let mut child_request_bodies: Vec<Option<String>> = vec![];
            for (prop_name, prop) in properties.iter() {
                let (inner, mut inner_diagnostics) =
                    resolve_schema(spec, prop, diagnostic_context);
                diagnostics.append(&mut inner_diagnostics);
                if inner.is_none() {
                    return (None, diagnostics);
                }
                let inner = inner.unwrap();
                let (request_body, mut inner_diagnostics) =
                    generate_request_body_from_schema(
                        spec,
                        inner,
                        Some(prop_name.to_string()),
                        diagnostic_context,
                        format!("{}.{}", jsonpath, prop_name).as_ref(),
                    );
                child_request_bodies.push(request_body);
                diagnostics.append(&mut inner_diagnostics);
            }
            let stringified_body = child_request_bodies
                .into_iter()
                .flatten()
                .collect::<Vec<String>>()
                .join(",\n");
            return match name {
                Some(name) => (
                    Some(format!("\"{}\": {{{}}}", name, stringified_body)),
                    diagnostics,
                ),
                None => (Some(format!("{{\n{}\n}}", stringified_body,)), diagnostics),
            };
        }
        Some(SchemaType::Array) => {
            let items = &schema.items;
            if items.is_none() {
                return (None, diagnostics);
            }
            let items = items.as_ref().unwrap();
            let inner = resolve_schema_from_schema(spec, items, diagnostic_context);
            match inner {
                (Some(inner), mut inner_diagnostics) => {
                    diagnostics.append(&mut inner_diagnostics);
                    let (child_request_body, mut child_diagnostics) =
                        generate_request_body_from_schema(
                            spec,
                            inner,
                            None,
                            diagnostic_context,
                            format!("{}[]", jsonpath).as_ref(),
                        );
                    diagnostics.append(&mut child_diagnostics);
                    if child_request_body.is_none() {
                        return (None, diagnostics);
                    }
                    let child_request_body = child_request_body.unwrap();
                    match name {
                        Some(name) => (
                            Some(format!("\"{}\": [{}]", name, child_request_body,)),
                            diagnostics,
                        ),
                        None => (Some(format!("[{}]", child_request_body)), diagnostics),
                    }
                }
                (None, mut inner_diagnostics) => {
                    diagnostics.append(&mut inner_diagnostics);
                    (None, diagnostics)
                }
            }
        }
        _ => {
            diagnostics.push(HeaveError::UnsupportedSchemaKind {
                context: diagnostic_context.clone(),
                kind: "Any".to_string(),
                jsonpath: name.unwrap_or("".to_string()),
            });
            (None, diagnostics)
        }
    }
}

fn resolve_response<'a>(
    spec: &'a Spec,
    response: &'a ObjectOrReference<Response>,
    diagnostic_context: &DiagnosticContext,
) -> (Option<&'a Response>, Vec<HeaveError>) {
    let mut diagnostics = vec![];
    match response {
        ObjectOrReference::Object(item) => (Some(item), diagnostics),
        ObjectOrReference::Ref { ref_path, .. } => {
            let response_name = ref_path.split("#/components/responses/").nth(1);
            if response_name.is_none() {
                diagnostics.push(HeaveError::MalformedResponseBodyReference {
                    context: diagnostic_context.clone(),
                    reference: ref_path.to_string(),
                });
                return (None, diagnostics);
            }
            let response_name = response_name.unwrap();
            let components = &spec.components;
            if components.is_none() {
                diagnostics.push(HeaveError::MissingComponents);
                return (None, diagnostics);
            }
            let found_response = components.as_ref().unwrap().responses.get(response_name);
            if found_response.is_none() {
                diagnostics.push(HeaveError::MissingResponseBodyReference {
                    context: diagnostic_context.clone(),
                    reference: ref_path.to_string(),
                });
                return (None, diagnostics);
            }
            let found_response = found_response.unwrap();
            match found_response {
                ObjectOrReference::Object(resp) => (Some(resp), diagnostics),
                ObjectOrReference::Ref { .. } => {
                    diagnostics.push(HeaveError::FailedResponseBodyDereference {
                        context: diagnostic_context.clone(),
                        reference: ref_path.to_string(),
                    });
                    (None, diagnostics)
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::{error::Error, path::PathBuf, str::FromStr};

    use insta::{assert_debug_snapshot, assert_snapshot, glob};
    use oas3::spec::Spec;

    use crate::{generate, write_outputs, Output, DEFAULT_HURL_TEMPLATE};

    // Creates a Spec from a file path
    macro_rules! spec_from_yaml {
        ($fname:expr) => {
            oas3::from_yaml(&std::fs::read_to_string($fname).unwrap()).unwrap()
        };
    }

    #[test]
    fn petstore() -> Result<(), Box<dyn Error>> {
        // Testing json and yaml in this same test so I make sure the output snapshots are the same
        let content = std::fs::read_to_string("src/snapshots/petstore/petstore.yaml")?;
        let spec: Spec = oas3::from_yaml(&content).expect("Could not deserialize input");
        let output_directory = PathBuf::from_str("src/snapshots/petstore")?;
        let result = generate(spec);
        write_outputs(&result.outputs, DEFAULT_HURL_TEMPLATE, &output_directory)?;
        let mut settings = insta::Settings::clone_current();
        settings.set_omit_expression(true);
        settings.bind(|| {
            insta::allow_duplicates! {
                glob!("snapshots/petstore/*.hurl", |path| {
                    let input = std::fs::read_to_string(path).unwrap();
                    assert_snapshot!(input);
                });
            };
        });

        let content = std::fs::read_to_string("src/snapshots/petstore/petstore.json")?;
        let spec: Spec = oas3::from_json(&content).expect("Could not deserialize input");
        let output_directory = PathBuf::from_str("src/snapshots/petstore")?;
        let result = generate(spec);
        write_outputs(&result.outputs, DEFAULT_HURL_TEMPLATE, &output_directory)?;
        let mut settings = insta::Settings::clone_current();
        settings.set_omit_expression(true);
        settings.bind(|| {
            insta::allow_duplicates! {
                glob!("snapshots/petstore/*.hurl", |path| {
                    let input = std::fs::read_to_string(path).unwrap();
                    assert_snapshot!(input);
                });
            }
        });

        Ok(())
    }

    #[test]
    fn diagnostic_inputs() -> Result<(), Box<dyn Error>> {
        let mut settings = insta::Settings::clone_current();
        settings.set_omit_expression(true);
        settings.bind(|| {
            glob!("snapshots/diagnostics/*.yaml", |path| {
                let input: Spec = spec_from_yaml!(&path);
                let result = generate(input);
                assert_debug_snapshot!(result);
            });
        });
        Ok(())
    }

    #[test]
    fn cycle_detection() -> Result<(), Box<dyn Error>> {
        let mut settings = insta::Settings::clone_current();
        settings.set_omit_expression(true);
        settings.bind(|| {
            glob!("snapshots/cycle_detection/*.yaml", |path| {
                let input: Spec = spec_from_yaml!(&path);
                let result = generate(input);
                assert_debug_snapshot!(result);
            });
        });
        Ok(())
    }

    #[test]
    fn read_only() -> Result<(), Box<dyn Error>> {
        let mut settings = insta::Settings::clone_current();
        settings.set_omit_expression(true);
        settings.bind(|| {
            glob!("snapshots/read_only/*.yaml", |path| {
                let input: Spec = spec_from_yaml!(&path);
                let result = generate(input);
                assert_debug_snapshot!(result);
            });
        });
        Ok(())
    }

    #[test]
    fn write_only() -> Result<(), Box<dyn Error>> {
        let mut settings = insta::Settings::clone_current();
        settings.set_omit_expression(true);
        settings.bind(|| {
            glob!("snapshots/write_only/*.yaml", |path| {
                let input: Spec = spec_from_yaml!(&path);
                let result = generate(input);
                assert_debug_snapshot!(result);
            });
        });
        Ok(())
    }

    #[test]
    fn allof_inputs() -> Result<(), Box<dyn Error>> {
        let spec: Spec = spec_from_yaml!("src/snapshots/allof/petstore.yaml");
        let output_directory = PathBuf::from_str("src/snapshots/allof")?;
        let result = generate(spec);
        write_outputs(&result.outputs, DEFAULT_HURL_TEMPLATE, &output_directory)?;
        let mut settings = insta::Settings::clone_current();
        settings.set_omit_expression(true);
        settings.bind(|| {
            glob!("snapshots/allof/*.hurl", |path| {
                let input = std::fs::read_to_string(path).unwrap();
                assert_snapshot!(input);
            });
        });
        Ok(())
    }

    #[test]
    fn filter_only_new_outputs() {
        let existing_files = vec![
            PathBuf::from("output/file1.hurl"),
            PathBuf::from("output/file3.hurl"),
        ];
        let out1 = Output {
            name: "file1.hurl".to_string(),
            method: "GET".to_string(),
            expected_status_code: 0,
            hurl_path: "".to_string(),
            oas_path: "".to_string(),
            oas_operation_id: None,
            header_parameters: vec![],
            query_parameters: vec![],
            asserts: vec![],
            request_body_parameter: "".to_string(),
        };
        let out2 = Output {
            name: "file2.hurl".to_string(),
            ..out1.clone()
        };
        let out3 = Output {
            name: "file3.hurl".to_string(),
            ..out1.clone()
        };
        let outputs = vec![out1, out2, out3];
        let filtered = crate::filter_only_new_outputs(&existing_files, outputs);
        assert_eq!(filtered.len(), 1);
        assert_eq!(filtered.first().unwrap().name, "file2.hurl");
    }

    #[test]
    fn filter_include_paths_outputs() {
        let regex = regex_lite::Regex::new("^/documents$").unwrap();
        let out1 = Output {
            name: "get_documents_200.hurl".to_string(),
            method: "GET".to_string(),
            expected_status_code: 200,
            hurl_path: "".to_string(),
            oas_path: "/documents".to_string(),
            oas_operation_id: None,
            header_parameters: vec![],
            query_parameters: vec![],
            asserts: vec![],
            request_body_parameter: "".to_string(),
        };
        let out2 = Output {
            name: "get_document_by_id_200.hurl".to_string(),
            oas_path: "/documents/{documentId}".to_string(),
            ..out1.clone()
        };
        let outputs = vec![out1, out2];
        let filtered = crate::filter_include_paths_outputs(regex, outputs);
        assert_eq!(filtered.len(), 1);
        assert_eq!(filtered.first().unwrap().name, "get_documents_200.hurl");
    }

    #[test]
    fn filter_include_status_codes_outputs() {
        let out1 = Output {
            name: "get_documents_200.hurl".to_string(),
            method: "GET".to_string(),
            expected_status_code: 200,
            hurl_path: "".to_string(),
            oas_path: "/documents".to_string(),
            oas_operation_id: None,
            header_parameters: vec![],
            query_parameters: vec![],
            asserts: vec![],
            request_body_parameter: "".to_string(),
        };
        let out2 = Output {
            name: "get_document_by_id_400.hurl".to_string(),
            expected_status_code: 400,
            ..out1.clone()
        };
        let out3 = Output {
            name: "get_document_by_id_401.hurl".to_string(),
            expected_status_code: 401,
            ..out1.clone()
        };
        let out4 = Output {
            name: "get_document_by_id_403.hurl".to_string(),
            expected_status_code: 403,
            ..out1.clone()
        };
        let out5 = Output {
            name: "get_document_by_id_404.hurl".to_string(),
            expected_status_code: 404,
            ..out1.clone()
        };
        let outputs = vec![
            out1.clone(),
            out2.clone(),
            out3.clone(),
            out4.clone(),
            out5.clone(),
        ];
        let regexes = vec![
            regex_lite::Regex::new("^2\\d{2}$").unwrap(),
            regex_lite::Regex::new("^4\\d{2}$").unwrap(),
            regex_lite::Regex::new("^200|400").unwrap(),
            regex_lite::Regex::new("^2[0-9]{2}$").unwrap(),
            regex_lite::Regex::new("[24]0[04]").unwrap(),
        ];
        let expected_outputs = [
            vec![&out1],
            vec![&out2, &out3, &out4, &out5],
            vec![&out1, &out2],
            vec![&out1],
            vec![&out1, &out2, &out5],
        ];
        for (regex, expected_output) in regexes.iter().zip(expected_outputs.iter()) {
            let filtered =
                crate::filter_include_status_codes_outputs(regex.clone(), outputs.clone());
            assert_eq!(filtered.len(), expected_output.len());
            for (i, output) in filtered.iter().enumerate() {
                assert_eq!(output.name, expected_output[i].name);
            }
        }
    }

    #[test]
    fn filter_include_operation_ids_outputs() {
        let out1 = Output {
            name: "addPet_200.hurl".to_string(),
            method: "".to_string(),
            expected_status_code: 200,
            hurl_path: "".to_string(),
            oas_path: "".to_string(),
            oas_operation_id: None,
            header_parameters: vec![],
            query_parameters: vec![],
            asserts: vec![],
            request_body_parameter: "".to_string(),
        };
        let out2 = Output {
            name: "updatePet_200.hurl".to_string(),
            oas_operation_id: Some("updatePet".to_string()),
            ..out1.clone()
        };
        let out3 = Output {
            name: "findPetsByStatus_200.hurl".to_string(),
            oas_operation_id: Some("findPetsByStatus".to_string()),
            ..out1.clone()
        };
        let out4 = Output {
            name: "findPetsByTags_200.hurl".to_string(),
            oas_operation_id: Some("findPetsByTags".to_string()),
            ..out1.clone()
        };
        let out5 = Output {
            name: "getPetById_200.hurl".to_string(),
            oas_operation_id: Some("getPetById".to_string()),
            ..out1.clone()
        };
        let outputs = vec![
            out1.clone(),
            out2.clone(),
            out3.clone(),
            out4.clone(),
            out5.clone(),
        ];
        let regexes = vec![
            regex_lite::Regex::new("Pets").unwrap(),
            regex_lite::Regex::new("^findPets").unwrap(),
            regex_lite::Regex::new("ById$").unwrap(),
            regex_lite::Regex::new("updatePet|getPetById").unwrap(),
        ];
        let expected_outputs = [
            vec![&out3, &out4],
            vec![&out3, &out4],
            vec![&out5],
            vec![&out2, &out5],
        ];
        for (regex, expected_output) in regexes.iter().zip(expected_outputs.iter()) {
            let filtered =
                crate::filter_include_operation_ids_outputs(regex.clone(), outputs.clone());
            assert_eq!(filtered.len(), expected_output.len());
            for (i, output) in filtered.iter().enumerate() {
                assert_eq!(output.name, expected_output[i].name);
            }
        }
    }
}
