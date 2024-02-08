use std::{error::Error, ops::Deref, path::PathBuf};

use clap::Parser;
use minijinja::{context, Environment};
use openapiv3::{OpenAPI, ReferenceOr};

/// Program to generate hurl files from openapi schemas
#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    /// Path to OAS file
    path: PathBuf,

    /// The directory where the files should be created
    output: PathBuf,
}

struct Output {
    expected_status_code: u16,
    name: String,
    path: String,
    method: String,
    header_parameters: Vec<String>,
    query_parameters: Vec<String>,
    asserts: Vec<String>,
}

fn main() -> Result<(), Box<dyn Error>> {
    let cli = Args::parse();
    let output_directory = cli.output;
    let output_directory_metadata = std::fs::metadata(&output_directory)?;
    if !output_directory_metadata.is_dir() {
        return Err("Output must be a directory".into());
    }

    let content = std::fs::read_to_string(&cli.path)?;

    let mut outputs: Vec<Output> = vec![];

    // TODO probably need to include header parameters and query parameters in GETs as well as
    // asserting the returned body
    // probably need to include sample request body for POST/PATCH
    let openapi: OpenAPI = serde_yaml::from_str(&content).expect("Could not deserialize input");
    for (path, method, operation) in openapi.operations() {
        let name = operation
            .operation_id
            .clone()
            .unwrap_or_else(|| format!("{}_{}", method, path.replace("/", "_")));
        let mut query_parameters: Vec<String> = vec![];
        let mut header_parameters: Vec<String> = vec![];
        for parameter in operation.parameters.iter() {
            match parameter {
                openapiv3::ReferenceOr::Reference { reference } => {
                    let parameter_name = reference.split("#/components/parameters/").nth(1);
                    if parameter_name.is_none() {
                        continue;
                    }
                    let parameter_name = parameter_name.unwrap();
                    let components = &openapi.components;
                    if components.is_none() {
                        continue;
                    }
                    let found_parameter =
                        components.as_ref().unwrap().parameters.get(parameter_name);
                    if found_parameter.is_none() {
                        continue;
                    }
                    let found_parameter = found_parameter.unwrap();
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

        for (status_code, response) in operation.responses.responses.iter() {
            let mut asserts: Vec<String> = vec![];
            match status_code {
                openapiv3::StatusCode::Range(_) => {
                    println!("Using ranges for status codes is not supported for responses.");
                }
                openapiv3::StatusCode::Code(code) => {
                    let name = format!("{}_{}.hurl", name, code);
                    match response {
                        openapiv3::ReferenceOr::Reference { reference } => {
                            let response_name = reference.split("#/components/responses/").nth(1);
                            if response_name.is_none() {
                                continue;
                            }
                            let response_name = response_name.unwrap();
                            let components = &openapi.components;
                            if components.is_none() {
                                continue;
                            }
                            let found_response =
                                components.as_ref().unwrap().responses.get(response_name);
                            if found_response.is_none() {
                                continue;
                            }
                            let found_response = found_response.unwrap();
                            if found_response.as_item().is_none() {
                                continue;
                            }
                            let found_response = found_response.as_item().unwrap();
                            let content = found_response.content.get("application/json");
                            if content.is_none() {
                                continue;
                            }
                            let schema = content.unwrap().schema.as_ref();
                            if schema.is_none() {
                                continue;
                            }
                            let schema = schema.unwrap().as_item().unwrap();
                            let mut new_asserts =
                                generate_assert_from_schema(&openapi, schema, "$");
                            asserts.append(&mut new_asserts);
                        }
                        openapiv3::ReferenceOr::Item(item) => {
                            let content = item.content.get("application/json");
                            if content.is_none() {
                                continue;
                            }
                            let schema = content.unwrap().schema.as_ref();
                            if schema.is_none() {
                                continue;
                            }
                            let schema = schema.unwrap();
                            let schema = resolve_schema(&openapi, schema);
                            if schema.is_none() {
                                continue;
                            }
                            let schema = schema.unwrap();
                            let mut new_asserts =
                                generate_assert_from_schema(&openapi, schema, "$");
                            asserts.append(&mut new_asserts);
                        }
                    };

                    let output = Output {
                        expected_status_code: *code,
                        name,
                        path: path.to_string().replace("{", "{{").replace("}", "}}"),
                        method: method.to_string().to_uppercase(),
                        header_parameters: header_parameters.clone(),
                        query_parameters: query_parameters.clone(),
                        asserts: asserts.clone(),
                    };
                    outputs.push(output)
                }
            };
        }
    }

    let mut jinja_env = Environment::new();
    jinja_env
        .add_template(
            "output.hurl",
            r#"{{ method }} {{ '{{ baseurl }}' }}{{ path | safe }}
Authorization: Bearer {{ '{{ authorization }}' }}
{% for header in header_parameters %}{{ header }}: 
{% endfor %}{% if query_parameters %}
[QueryStringParams]
{% for query in query_parameters %}{{ query }}: 
{% endfor %}{% endif %}
HTTP {{ expected_status_code }}
{% if asserts %}
[Asserts]
{% for assert in asserts %}{{ assert }}
{% endfor %}{% endif %}

"#,
        )
        .unwrap();
    let template = jinja_env.get_template("output.hurl").unwrap();

    for output in outputs.iter() {
        let content = template
            .render(context! {
                method => output.method,
                path => output.path,
                expected_status_code => output.expected_status_code,
                header_parameters => output.header_parameters,
                query_parameters => output.query_parameters,
                asserts => output.asserts,
            })
            .unwrap();
        let mut file_path = output_directory.clone();
        file_path.push(&output.name);
        std::fs::write(file_path, content)?;
    }
    Ok(())
}

fn generate_assert_from_schema(
    openapi: &openapiv3::OpenAPI,
    schema: &openapiv3::Schema,
    jsonpath: &str,
) -> Vec<String> {
    let mut asserts = vec![];
    if let openapiv3::SchemaKind::Type(schema_type) = &schema.schema_kind {
        match schema_type {
            openapiv3::Type::String(_) => {
                asserts.push(format!("jsonpath \"{}\" isString", jsonpath))
            }
            openapiv3::Type::Number(_) => {
                asserts.push(format!("jsonpath \"{}\" isNumber", jsonpath))
            }
            openapiv3::Type::Integer(_) => {
                asserts.push(format!("jsonpath \"{}\" isInteger", jsonpath))
            }
            openapiv3::Type::Object(ob) => {
                asserts.push(format!("jsonpath \"{}\" isCollection", jsonpath));
                let properties = &ob.properties;
                for (name, prop) in properties.iter() {
                    let inner = resolve_schema_box(openapi, prop);
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
                    let mut child_asserts =
                        generate_assert_from_schema(openapi, inner, inner_jsonpath.as_ref());
                    asserts.append(&mut child_asserts);
                }
            }
            openapiv3::Type::Array(_) => {
                asserts.push(format!("jsonpath \"{}\" isCollection", jsonpath))
            }
            openapiv3::Type::Boolean(_) => {
                asserts.push(format!("jsonpath \"{}\" isBoolean", jsonpath))
            }
        }
    } else {
        println!("Only explicit types for responses are supported. Using AnyOf, Allof, etc. is not supported.");
    }
    asserts
}

fn resolve_schema_box<'a>(
    openapi: &'a openapiv3::OpenAPI,
    schema: &'a openapiv3::ReferenceOr<Box<openapiv3::Schema>>,
) -> Option<&'a openapiv3::Schema> {
    match schema {
        ReferenceOr::Item(item) => {
            return Some(item.deref());
        }
        ReferenceOr::Reference { reference } => {
            let schema_name = reference.split("#/components/schemas/").nth(1);
            if schema_name.is_none() {
                return None;
            }
            let schema_name = schema_name.unwrap();
            let components = &openapi.components;
            if components.is_none() {
                return None;
            }
            let found_schema = components.as_ref().unwrap().schemas.get(schema_name);
            if found_schema.is_none() {
                return None;
            }
            let found_schema = found_schema.unwrap();
            if found_schema.as_item().is_none() {
                return None;
            }
            let schema = found_schema.as_item().unwrap();
            Some(schema)
        }
    }
}

fn resolve_schema<'a>(
    openapi: &'a openapiv3::OpenAPI,
    schema: &'a openapiv3::ReferenceOr<openapiv3::Schema>,
) -> Option<&'a openapiv3::Schema> {
    match schema {
        ReferenceOr::Item(item) => {
            return Some(item);
        }
        ReferenceOr::Reference { reference } => {
            let schema_name = reference.split("#/components/schemas/").nth(1);
            if schema_name.is_none() {
                return None;
            }
            let schema_name = schema_name.unwrap();
            let components = &openapi.components;
            if components.is_none() {
                return None;
            }
            let found_schema = components.as_ref().unwrap().schemas.get(schema_name);
            if found_schema.is_none() {
                return None;
            }
            let found_schema = found_schema.unwrap();
            if found_schema.as_item().is_none() {
                return None;
            }
            let schema = found_schema.as_item().unwrap();
            Some(schema)
        }
    }
}
