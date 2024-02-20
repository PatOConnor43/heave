use clap::{Args, Parser, Subcommand};
use minijinja::{context, Environment};
use openapiv3::{MediaType, OpenAPI, ReferenceOr};
use std::{error::Error, path::PathBuf};

/// Program to generate hurl files from openapi schemas
#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Debug, Subcommand)]
enum Commands {
    #[command(about = "Generate hurl files from OpenAPI spec.")]
    Generate(GenerateArgs),

    #[command(about = "Print the default template.")]
    Template,
}

#[derive(Args, Debug)]
struct GenerateArgs {
    #[arg(
        help = "The path to an OpenAPI spec. This spec must not contain references to other files."
    )]
    path: PathBuf,

    #[arg(help = "The directory where generated hurl files will be created.")]
    output: PathBuf,

    #[arg(long, help = "Prints the default template.")]
    template: Option<PathBuf>,
}

/// The struct used to capture output variables.
///
/// Each field defined in this struct will be available to the template. The template uses the
/// minijinja syntax.
pub struct Output {
    pub expected_status_code: u16,
    pub name: String,
    pub path: String,
    pub method: String,
    pub header_parameters: Vec<String>,
    pub query_parameters: Vec<String>,
    pub asserts: Vec<String>,
    pub request_body_parameter: String,
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
{% endfor %}{% endif %}{{ request_body_parameter }}
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
UnsupportedRequestBodySchemaKindUnsupported

Message: RequestBody Schema kind. Using AllOf, AnyOf, etc. is currently not supported.
Path: {}
Operation: {}
Detected Kind: {}
JSON path: {}"#,
.context.path, .context.operation, .kind, .jsonpath
    )]
    UnsupportedRequestBodySchemaKind {
        context: DiagnosticContext,
        kind: String,
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
            let output_directory = args.output;
            let output_directory_metadata = std::fs::metadata(&output_directory)?;
            if !output_directory_metadata.is_dir() {
                return Err("Output must be a directory".into());
            }

            let template = match &args.template {
                Some(t) => {
                    let metadata = std::fs::metadata(&t)?;
                    if !metadata.is_file() {
                        return Err("Template must be a file".into());
                    }
                    let template_content = std::fs::read_to_string(t);
                    let template_content = template_content.unwrap();
                    template_content
                }
                None => DEFAULT_HURL_TEMPLATE.to_string(),
            };
            let input_path = &args.path;
            let input_metadata = std::fs::metadata(&input_path)?;
            if !input_metadata.is_file() {
                return Err("Input spec must be a file".into());
            }
            let input_extension = match &input_path.extension() {
                Some(ext) => match ext.to_str() {
                    Some("json") => Ok(InputSpecExtension::Json),
                    Some("yaml") => Ok(InputSpecExtension::Yaml),
                    _ => Err("Input spec must be json or yaml file"),
                },
                None => Err("Input spec must be json or yaml file"),
            }?;

            let content = std::fs::read_to_string(&input_path)?;
            let openapi: OpenAPI = match input_extension {
                InputSpecExtension::Json => {
                    serde_json::from_str(&content).expect("Could not deserialize input as json")
                }
                InputSpecExtension::Yaml => {
                    serde_yaml::from_str(&content).expect("Could not deserialize input as yaml")
                }
            };

            generate(openapi, output_directory, &template)
        }
        Commands::Template => {
            println!("{}", DEFAULT_HURL_TEMPLATE);
            Ok(())
        }
    }
}

fn generate(
    openapi: openapiv3::OpenAPI,
    output_directory: PathBuf,
    template: &str,
) -> Result<(), Box<dyn Error>> {
    let mut jinja_env = Environment::new();
    jinja_env
        .add_template("output.hurl", template)
        .map_err(|e| HeaveError::JinjaError { source: e })?;
    let mut outputs: Vec<Output> = vec![];
    let mut diagnostics: Vec<HeaveError> = vec![];
    for (path, method, operation) in openapi.operations() {
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
                openapiv3::ReferenceOr::Reference { reference } => {
                    let parameter_name = reference.split("#/components/parameters/").nth(1);
                    if parameter_name.is_none() {
                        diagnostics.push(HeaveError::MalformedParameterReference {
                            operation: name.to_string(),
                            path: path.to_string(),
                            reference: reference.to_string(),
                        });
                        continue;
                    }
                    let parameter_name = parameter_name.unwrap();
                    let components = &openapi.components;
                    if components.is_none() {
                        diagnostics.push(HeaveError::MissingComponents);
                        continue;
                    }
                    let found_parameter =
                        components.as_ref().unwrap().parameters.get(parameter_name);
                    if found_parameter.is_none() {
                        diagnostics.push(HeaveError::MissingParameterReference {
                            context: context.clone(),
                            reference: reference.to_string(),
                        });
                        continue;
                    }
                    let found_parameter = found_parameter.unwrap();
                    // TODO add support for reference parameters
                    if found_parameter.as_item().is_none() {
                        continue;
                    }
                    let found_parameter = found_parameter.as_item().unwrap();
                    match found_parameter {
                        openapiv3::Parameter::Query { parameter_data, .. } => {
                            query_parameters.push(parameter_data.name.to_string());
                        }
                        openapiv3::Parameter::Header { parameter_data, .. } => {
                            header_parameters.push(parameter_data.name.to_string());
                        }
                        _ => {}
                    }
                }
                openapiv3::ReferenceOr::Item(item) => match item {
                    openapiv3::Parameter::Query { parameter_data, .. } => {
                        query_parameters.push(parameter_data.name.to_string());
                    }
                    openapiv3::Parameter::Header { parameter_data, .. } => {
                        header_parameters.push(parameter_data.name.to_string());
                    }
                    _ => {}
                },
            }
        }

        if let Some(request_body) = &operation.request_body {
            let (request_body, mut inner_diagnostics) =
                resolve_request_body(&openapi, &request_body, &context);
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
                diagnostics
                    .push(HeaveError::MissingApplicationJsonRequestBodyMediaType { context });
                break;
            }
            let media_type = media_type.unwrap();
            let schema = &media_type.schema;
            if schema.is_none() {
                diagnostics.push(HeaveError::MissingSchemaDefinitionForMediaType { context });
                break;
            }
            let schema = schema.as_ref().unwrap();
            let (schema, mut inner_diagnostics) = resolve_schema(&openapi, &schema, &context);
            diagnostics.append(&mut inner_diagnostics);
            if schema.is_none() {
                break;
            }
            let schema = schema.unwrap();
            let request_body_parameter_tuple =
                generate_request_body_from_schema(&openapi, &schema, None, &context);
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
        }

        for (status_code, response) in operation.responses.responses.iter() {
            let mut asserts: Vec<String> = vec![];
            match status_code {
                openapiv3::StatusCode::Range(_) => {
                    println!("Using ranges for status codes is not supported for responses.");
                }
                openapiv3::StatusCode::Code(code) => {
                    let name = format!("{}_{}.hurl", name, code);
                    let response = resolve_response(&openapi, response);
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
                        continue;
                    }
                    let schema = media_type.unwrap().schema.as_ref();
                    if schema.is_none() {
                        continue;
                    }
                    let schema = schema.unwrap();
                    let (schema, mut inner_diagnostics) =
                        resolve_schema(&openapi, schema, &context);
                    diagnostics.append(&mut inner_diagnostics);
                    if schema.is_none() {
                        continue;
                    }
                    let schema = schema.unwrap();
                    let is_required = true;
                    let (mut new_asserts, mut new_diagnostics) =
                        generate_assert_from_schema(&openapi, schema, "$", is_required, &context);
                    asserts.append(&mut new_asserts);
                    diagnostics.append(&mut new_diagnostics);

                    let output = Output {
                        expected_status_code: *code,
                        name,
                        path: path.to_string().replace("{", "{{").replace("}", "}}"),
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
            };
        }
    }

    let template = jinja_env.get_template("output.hurl").unwrap();

    for output in outputs.iter() {
        let content = template
            .render(context! {
                name => output.name,
                method => output.method,
                path => output.path,
                expected_status_code => output.expected_status_code,
                header_parameters => output.header_parameters,
                query_parameters => output.query_parameters,
                asserts => output.asserts,
                request_body_parameter => output.request_body_parameter,
            })
            .unwrap();
        let mut file_path = output_directory.clone();
        file_path.push(&output.name);
        std::fs::write(file_path, content)?;
    }
    diagnostics.iter().for_each(|d| println!("{}", d));
    Ok(())
}

fn generate_assert_from_schema(
    openapi: &openapiv3::OpenAPI,
    schema: &openapiv3::Schema,
    jsonpath: &str,
    is_required: bool,
    diagnostic_context: &DiagnosticContext,
) -> (Vec<String>, Vec<HeaveError>) {
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
    match &schema.schema_kind {
        openapiv3::SchemaKind::OneOf { .. } => {
            diagnostics.push(HeaveError::UnsupportedRequestBodySchemaKind {
                context: diagnostic_context.clone(),
                kind: "OneOf".to_string(),
                jsonpath: jsonpath.to_string(),
            })
        }
        openapiv3::SchemaKind::AllOf { .. } => {
            diagnostics.push(HeaveError::UnsupportedRequestBodySchemaKind {
                context: diagnostic_context.clone(),
                kind: "AllOf".to_string(),
                jsonpath: jsonpath.to_string(),
            })
        }
        openapiv3::SchemaKind::AnyOf { .. } => {
            diagnostics.push(HeaveError::UnsupportedRequestBodySchemaKind {
                context: diagnostic_context.clone(),
                kind: "AnyOf".to_string(),
                jsonpath: jsonpath.to_string(),
            })
        }
        openapiv3::SchemaKind::Not { .. } => {
            diagnostics.push(HeaveError::UnsupportedRequestBodySchemaKind {
                context: diagnostic_context.clone(),
                kind: "Not".to_string(),
                jsonpath: jsonpath.to_string(),
            })
        }
        openapiv3::SchemaKind::Any(_) => {
            diagnostics.push(HeaveError::UnsupportedRequestBodySchemaKind {
                context: diagnostic_context.clone(),
                kind: "Any".to_string(),
                jsonpath: jsonpath.to_string(),
            })
        }
        openapiv3::SchemaKind::Type(schema_type) => {
            match schema_type {
                openapiv3::Type::Boolean(_) => {
                    asserts.push(is_required_formatter(&jsonpath, "isBoolean", is_required))
                }
                openapiv3::Type::String(_) => {
                    asserts.push(is_required_formatter(&jsonpath, "isString", is_required))
                }
                openapiv3::Type::Number(_) => {
                    asserts.push(is_required_formatter(&jsonpath, "isNumber", is_required))
                }
                openapiv3::Type::Integer(_) => {
                    asserts.push(is_required_formatter(&jsonpath, "isInteger", is_required))
                }
                openapiv3::Type::Array(a) => {
                    asserts.push(is_required_formatter(
                        &jsonpath,
                        "isCollection",
                        is_required,
                    ));
                    let items = &a.items;
                    if items.is_none() {
                        return (asserts, diagnostics);
                    }
                    let items = items.as_ref().unwrap();
                    let unboxed = items.clone().unbox();
                    let (inner, mut inner_diagnostics) =
                        resolve_schema(openapi, &unboxed, &diagnostic_context);
                    diagnostics.append(&mut inner_diagnostics);
                    if inner.is_none() {
                        return (asserts, diagnostics);
                    }
                    let inner = inner.unwrap();

                    // Take the existing path and index the first element in the list.
                    let inner_jsonpath = format!("{}[0]", jsonpath);

                    // is_required is always false because a list may always be empty
                    let is_required = false;

                    let (mut child_asserts, mut child_diagnostics) = generate_assert_from_schema(
                        openapi,
                        inner,
                        inner_jsonpath.as_ref(),
                        is_required,
                        diagnostic_context,
                    );
                    asserts.append(&mut child_asserts);
                    diagnostics.append(&mut child_diagnostics);
                }
                openapiv3::Type::Object(ob) => {
                    asserts.push(is_required_formatter(
                        &jsonpath,
                        "isCollection",
                        is_required,
                    ));
                    let properties = &ob.properties;
                    for (name, prop) in properties.iter() {
                        let unboxed = prop.clone().unbox();
                        let (inner, mut inner_diagnostics) =
                            resolve_schema(openapi, &unboxed, diagnostic_context);
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
                        let child_is_required = is_required && ob.required.contains(name);
                        let (mut child_asserts, mut child_diagnostics) =
                            generate_assert_from_schema(
                                openapi,
                                inner,
                                inner_jsonpath.as_ref(),
                                child_is_required,
                                diagnostic_context,
                            );
                        asserts.append(&mut child_asserts);
                        diagnostics.append(&mut child_diagnostics);
                    }
                }
            }
        }
    }
    (asserts, diagnostics)
}

fn resolve_schema<'a>(
    openapi: &'a openapiv3::OpenAPI,
    schema: &'a openapiv3::ReferenceOr<openapiv3::Schema>,
    diagnostic_context: &DiagnosticContext,
) -> (Option<&'a openapiv3::Schema>, Vec<HeaveError>) {
    let mut diagnostics: Vec<HeaveError> = vec![];
    match schema {
        ReferenceOr::Item(item) => {
            return (Some(item), diagnostics);
        }
        ReferenceOr::Reference { reference } => {
            let schema_name = reference.split("#/components/schemas/").nth(1);
            if schema_name.is_none() {
                diagnostics.push(HeaveError::MalformedSchemaReference {
                    context: diagnostic_context.clone(),
                    reference: reference.to_string(),
                });
                return (None, diagnostics);
            }
            let schema_name = schema_name.unwrap();
            let components = &openapi.components;
            if components.is_none() {
                diagnostics.push(HeaveError::MissingComponents);
                return (None, diagnostics);
            }
            let found_schema = components.as_ref().unwrap().schemas.get(schema_name);
            if found_schema.is_none() {
                diagnostics.push(HeaveError::MissingSchemaReference {
                    context: diagnostic_context.clone(),
                    reference: reference.to_string(),
                });
                return (None, diagnostics);
            }
            let found_schema = found_schema.unwrap();
            if found_schema.as_item().is_none() {
                diagnostics.push(HeaveError::FailedSchemaDereference {
                    context: diagnostic_context.clone(),
                    reference: reference.to_string(),
                });
                return (None, diagnostics);
            }
            let schema = found_schema.as_item().unwrap();
            (Some(schema), diagnostics)
        }
    }
}

fn resolve_request_body<'a>(
    openapi: &'a openapiv3::OpenAPI,
    request_body: &'a openapiv3::ReferenceOr<openapiv3::RequestBody>,
    diagnostic_context: &DiagnosticContext,
) -> (Option<&'a openapiv3::RequestBody>, Vec<HeaveError>) {
    let mut diagnostics: Vec<HeaveError> = vec![];
    match request_body {
        ReferenceOr::Item(item) => {
            return (Some(item), diagnostics);
        }
        ReferenceOr::Reference { reference } => {
            let request_body_name = reference.split("#/components/requestBodies/").nth(1);
            if request_body_name.is_none() {
                diagnostics.push(HeaveError::MalformedRequestBodyReference {
                    context: diagnostic_context.clone(),
                    reference: reference.to_string(),
                });
                return (None, diagnostics);
            }
            let request_body_name = request_body_name.unwrap();
            let components = &openapi.components;
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
                    reference: reference.to_string(),
                });
                return (None, diagnostics);
            }
            let found_request_body = found_request_body.unwrap();
            if found_request_body.as_item().is_none() {
                diagnostics.push(HeaveError::FailedRequestBodyDereference {
                    context: diagnostic_context.clone(),
                    reference: reference.to_string(),
                });
                return (None, diagnostics);
            }
            let request_body = found_request_body.as_item().unwrap();
            (Some(request_body), diagnostics)
        }
    }
}

fn generate_request_body_from_schema(
    openapi: &openapiv3::OpenAPI,
    schema: &openapiv3::Schema,
    name: Option<String>,
    diagnostic_context: &DiagnosticContext,
) -> (Option<String>, Vec<HeaveError>) {
    let mut diagnostics = vec![];
    match &schema.schema_kind {
        openapiv3::SchemaKind::OneOf { .. } => {
            diagnostics.push(HeaveError::UnsupportedRequestBodySchemaKind {
                context: diagnostic_context.clone(),
                kind: "OneOf".to_string(),
                jsonpath: name.unwrap_or("".to_string()),
            })
        }
        openapiv3::SchemaKind::AllOf { .. } => {
            diagnostics.push(HeaveError::UnsupportedRequestBodySchemaKind {
                context: diagnostic_context.clone(),
                kind: "AllOf".to_string(),
                jsonpath: name.unwrap_or("".to_string()),
            })
        }
        openapiv3::SchemaKind::AnyOf { .. } => {
            diagnostics.push(HeaveError::UnsupportedRequestBodySchemaKind {
                context: diagnostic_context.clone(),
                kind: "AnyOf".to_string(),
                jsonpath: name.unwrap_or("".to_string()),
            })
        }
        openapiv3::SchemaKind::Not { .. } => {
            diagnostics.push(HeaveError::UnsupportedRequestBodySchemaKind {
                context: diagnostic_context.clone(),
                kind: "Not".to_string(),
                jsonpath: name.unwrap_or("".to_string()),
            })
        }
        openapiv3::SchemaKind::Any(_) => {
            diagnostics.push(HeaveError::UnsupportedRequestBodySchemaKind {
                context: diagnostic_context.clone(),
                kind: "Any".to_string(),
                jsonpath: name.unwrap_or("".to_string()),
            })
        }
        openapiv3::SchemaKind::Type(schema_type) => {
            // A small helper that takes properties that may or may not have names and formats them
            // accordingly. If they have a name, start by indenting them, print the named property,
            // then give it a default value. If there is no name, just print the default value.
            let single_property_formatter = |name: Option<String>, default: &str| -> String {
                match name {
                    Some(name) => format!("\"{}\": {}", name, default),
                    None => format!("{}", default),
                }
            };
            return match schema_type {
                openapiv3::Type::Boolean(_) => {
                    (Some(single_property_formatter(name, "false")), diagnostics)
                }
                openapiv3::Type::String(_) => {
                    (Some(single_property_formatter(name, "\"\"")), diagnostics)
                }
                openapiv3::Type::Number(_) | openapiv3::Type::Integer(_) => {
                    (Some(single_property_formatter(name, "0")), diagnostics)
                }
                openapiv3::Type::Object(ob) => {
                    let properties = &ob.properties;
                    let mut child_request_bodies: Vec<Option<String>> = vec![];
                    for (name, prop) in properties.iter() {
                        let unboxed = prop.clone().unbox();
                        let (inner, mut inner_diagnostics) =
                            resolve_schema(&openapi, &unboxed, diagnostic_context);
                        diagnostics.append(&mut inner_diagnostics);
                        if inner.is_none() {
                            return (None, diagnostics);
                        }
                        let inner = inner.unwrap();
                        let (request_body, mut inner_diagnostics) =
                            generate_request_body_from_schema(
                                &openapi,
                                &inner,
                                Some(name.to_string()),
                                diagnostic_context,
                            );
                        child_request_bodies.push(request_body);
                        diagnostics.append(&mut inner_diagnostics);
                    }
                    let stringified_body = child_request_bodies
                        .into_iter()
                        .filter_map(|body| body)
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
                openapiv3::Type::Array(array) => {
                    let items = &array.items;
                    if items.is_none() {
                        return (None, diagnostics);
                    }
                    let items = items.as_ref().unwrap();
                    let unboxed = items.clone().unbox();
                    let (inner, mut inner_diagnostics) =
                        resolve_schema(&openapi, &unboxed, diagnostic_context);
                    diagnostics.append(&mut inner_diagnostics);
                    if inner.is_none() {
                        return (None, diagnostics);
                    }
                    let inner = inner.unwrap();
                    let (child_request_body, mut child_diagnostics) =
                        generate_request_body_from_schema(
                            &openapi,
                            &inner,
                            None,
                            diagnostic_context,
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
            };
        }
    }
    (None, diagnostics)
}

fn resolve_response<'a>(
    openapi: &'a openapiv3::OpenAPI,
    response: &'a openapiv3::ReferenceOr<openapiv3::Response>,
) -> Option<&'a openapiv3::Response> {
    match response {
        ReferenceOr::Item(item) => {
            return Some(item);
        }
        ReferenceOr::Reference { reference } => {
            let response_name = reference.split("#/components/responses/").nth(1);
            if response_name.is_none() {
                return None;
            }
            let response_name = response_name.unwrap();
            let components = &openapi.components;
            if components.is_none() {
                return None;
            }
            let found_response = components.as_ref().unwrap().responses.get(response_name);
            if found_response.is_none() {
                return None;
            }
            let found_response = found_response.unwrap();
            if found_response.as_item().is_none() {
                return None;
            }
            let schema = found_response.as_item().unwrap();
            Some(schema)
        }
    }
}

#[cfg(test)]
mod tests {
    use std::{error::Error, path::PathBuf, str::FromStr};

    use insta::{assert_snapshot, glob};
    use openapiv3::OpenAPI;

    use crate::generate;

    #[test]
    fn petstore() -> Result<(), Box<dyn Error>> {
        // Testing json and yaml in this same test so I make sure the output snapshots are the same
        let content = std::fs::read_to_string("src/snapshots/petstore/petstore.yaml")?;
        let openapi: OpenAPI = serde_yaml::from_str(&content).expect("Could not deserialize input");
        let output_directory = PathBuf::from_str("src/snapshots/petstore")?;
        generate(openapi, output_directory, crate::DEFAULT_HURL_TEMPLATE)?;
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
        let openapi: OpenAPI = serde_json::from_str(&content).expect("Could not deserialize input");
        let output_directory = PathBuf::from_str("src/snapshots/petstore")?;
        generate(openapi, output_directory, crate::DEFAULT_HURL_TEMPLATE)?;
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
}
