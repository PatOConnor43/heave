use std::{error::Error, path::PathBuf};

use clap::Parser;
use minijinja::{context, Environment};
use openapiv3::OpenAPI;

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
        dbg!(&query_parameters);

        for (status_code, _) in operation.responses.responses.iter() {
            match status_code {
                openapiv3::StatusCode::Range(_) => {
                    println!("Using ranges for status codes is not supported for responses.");
                }
                openapiv3::StatusCode::Code(code) => {
                    let name = format!("{}_{}.hurl", name, code);
                    let output = Output {
                        expected_status_code: *code,
                        name,
                        path: path.to_string().replace("{", "{{").replace("}", "}}"),
                        method: method.to_string().to_uppercase(),
                        header_parameters: header_parameters.clone(),
                        query_parameters: query_parameters.clone(),
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
                query_parameters => output.query_parameters
            })
            .unwrap();
        let mut file_path = output_directory.clone();
        file_path.push(&output.name);
        std::fs::write(file_path, content)?;
    }
    Ok(())
}
