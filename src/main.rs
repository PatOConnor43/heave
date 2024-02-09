use std::{error::Error, ops::Deref, path::PathBuf};

use clap::Parser;
use minijinja::{context, Environment};
use openapiv3::{OpenAPI, ReferenceOr};

/// Program to generate hurl files from openapi schemas
#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    #[arg(
        help = "The path to an OpenAPI spec. This spec must not contain references to other files."
    )]
    path: PathBuf,

    #[arg(help = "The directory where generated hurl files will be created.")]
    output: PathBuf,
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

fn main() -> Result<(), Box<dyn Error>> {
    let cli = Args::parse();
    let output_directory = cli.output;
    let output_directory_metadata = std::fs::metadata(&output_directory)?;
    if !output_directory_metadata.is_dir() {
        return Err("Output must be a directory".into());
    }

    let content = std::fs::read_to_string(&cli.path)?;

    let mut outputs: Vec<Output> = vec![];

    let openapi: OpenAPI = serde_yaml::from_str(&content).expect("Could not deserialize input");
    for (path, method, operation) in openapi.operations() {
        let name = operation
            .operation_id
            .clone()
            .unwrap_or_else(|| format!("{}_{}", method, path.replace("/", "_")));
        let mut query_parameters: Vec<String> = vec![];
        let mut header_parameters: Vec<String> = vec![];
        let mut request_body_parameter: Option<String> = None;
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

        if let Some(request_body) = &operation.request_body {
            let request_body = resolve_request_body(&openapi, &request_body);
            if request_body.is_none() {
                break;
            }
            let request_body = request_body.unwrap();
            let content = &request_body.content;
            let media_type = content.get("application/json");
            if media_type.is_none() {
                break;
            }
            let media_type = media_type.unwrap();
            let schema = &media_type.schema;
            if schema.is_none() {
                break;
            }
            let schema = schema.as_ref().unwrap();
            let schema = resolve_schema(&openapi, &schema);
            if schema.is_none() {
                break;
            }
            let schema = schema.unwrap();
            request_body_parameter = generate_request_body_from_schema(&openapi, &schema, None, 0);
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
                        request_body_parameter: request_body_parameter
                            .clone()
                            .unwrap_or("".to_string()),
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

"#,
        )
        .unwrap();
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

fn resolve_request_body<'a>(
    openapi: &'a openapiv3::OpenAPI,
    request_body: &'a openapiv3::ReferenceOr<openapiv3::RequestBody>,
) -> Option<&'a openapiv3::RequestBody> {
    match request_body {
        ReferenceOr::Item(item) => {
            return Some(item);
        }
        ReferenceOr::Reference { reference } => {
            let request_body_name = reference.split("#/components/requestBodies/").nth(1);
            if request_body_name.is_none() {
                return None;
            }
            let request_body_name = request_body_name.unwrap();
            let components = &openapi.components;
            if components.is_none() {
                return None;
            }
            let found_request_body = components
                .as_ref()
                .unwrap()
                .request_bodies
                .get(request_body_name);
            if found_request_body.is_none() {
                return None;
            }
            let found_request_body = found_request_body.unwrap();
            if found_request_body.as_item().is_none() {
                return None;
            }
            let request_body = found_request_body.as_item().unwrap();
            Some(request_body)
        }
    }
}

fn generate_request_body_from_schema(
    openapi: &openapiv3::OpenAPI,
    schema: &openapiv3::Schema,
    name: Option<String>,
    level: usize,
) -> Option<String> {
    if let openapiv3::SchemaKind::Type(schema_type) = &schema.schema_kind {
        return match schema_type {
            openapiv3::Type::String(_) => {
                Some(format!("{}\"{}\": \"\"", " ".repeat(level), name.unwrap()))
            }
            openapiv3::Type::Number(_) => {
                Some(format!("{}\"{}\": 0", " ".repeat(level), name.unwrap()))
            }
            openapiv3::Type::Integer(_) => {
                Some(format!("{}\"{}\": 0", " ".repeat(level), name.unwrap()))
            }
            openapiv3::Type::Object(ob) => {
                let properties = &ob.properties;
                let mut child_request_bodies: Vec<Option<String>> = vec![];
                for (name, prop) in properties.iter() {
                    let inner = resolve_schema_box(&openapi, &prop);
                    if inner.is_none() {
                        return None;
                    }
                    let inner = inner.unwrap();
                    let request_body = generate_request_body_from_schema(
                        &openapi,
                        &inner,
                        Some(name.to_string()),
                        level + 2,
                    );
                    child_request_bodies.push(request_body);
                }
                let stringified_body = child_request_bodies
                    .into_iter()
                    .filter_map(|body| body)
                    .collect::<Vec<String>>()
                    .join(",\n");
                return match name {
                    Some(name) => Some(format!(
                        "{}\"{}\": {{{}}}",
                        " ".repeat(level),
                        name,
                        stringified_body
                    )),
                    None => Some(format!(
                        "{}{{\n{}\n{}}}",
                        " ".repeat(level),
                        stringified_body,
                        " ".repeat(level)
                    )),
                };
            }
            openapiv3::Type::Array(array) => {
                let items = &array.items;
                if items.is_none() {
                    return None;
                }
                let items = items.as_ref().unwrap();
                let inner = resolve_schema_box(&openapi, &items);
                if inner.is_none() {
                    return None;
                }
                let inner = inner.unwrap();
                let child_request_body =
                    generate_request_body_from_schema(&openapi, &inner, None, level + 2);
                if child_request_body.is_none() {
                    return None;
                }
                let child_request_body = child_request_body.unwrap();
                match name {
                    Some(name) => Some(format!(
                        "{}\"{}\": [{}]",
                        " ".repeat(level),
                        name,
                        child_request_body
                    )),
                    None => Some(format!(
                        "{}[\n{}\n{}]",
                        " ".repeat(level),
                        child_request_body,
                        " ".repeat(level)
                    )),
                }
            }
            openapiv3::Type::Boolean(_) => {
                Some(format!("{}\"{}\": true", " ".repeat(level), name.unwrap()))
            }
        };
    } else {
        println!("Only explicit types for responses are supported. Using AnyOf, Allof, etc. is not supported.");
    }
    None
}
