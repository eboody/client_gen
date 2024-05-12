use std::{
    borrow::Borrow,
    cell::RefCell,
    collections::{HashMap, HashSet},
    fs,
    path::PathBuf,
    rc::Rc,
};

use regex::{Regex, RegexBuilder};

use crate::{directories::Directory, util::camel_to_snake, Error, Result};

#[derive(Debug, Clone)]
pub struct OutputFileContent {
    imports: Vec<String>,
    bindings: Vec<String>,
    pub functions: Vec<EntityFunction>,
}

impl OutputFileContent {
    pub fn new<'a>(directory: &'a Directory) -> Result<Self> {
        let output_file = Rc::new(RefCell::new(OutputFileContent {
            imports: vec![],
            bindings: vec![],
            functions: vec![],
        }));

        Self::get_bindings(directory, output_file.clone())?;
        Self::populate_from_dir(directory, output_file)
    }

    pub fn get_bindings<'a>(
        directory: &'a Directory,
        output_file: Rc<RefCell<OutputFileContent>>,
    ) -> Result<Self> {
        for file in &directory.files {
            let path = file.to_path_buf();

            if is_bindings_file(&path) {
                let mut of = output_file.borrow_mut();
                of.bindings.append(&mut get_bindings(&path));
            }
        }

        for dir in &directory.directories {
            Self::get_bindings(dir, Rc::clone(&output_file))?;
        }

        Ok((*output_file).clone().into_inner())
    }

    fn populate_from_dir<'a>(
        directory: &'a Directory,
        output_file: Rc<RefCell<OutputFileContent>>,
    ) -> Result<Self> {
        for file in &directory.files {
            let path = file.to_path_buf();

            if is_rpc_file(&path) {
                let mut of = output_file.borrow_mut();
                let bindings = of.bindings.clone();
                of.functions
                    .append(&mut process_rpc_file(&path, &bindings)?);
            }
        }

        for dir in &directory.directories {
            OutputFileContent::populate_from_dir(dir, Rc::clone(&output_file))?;
        }

        Ok((*output_file).clone().into_inner())
    }

    pub fn write_to_file(self, client_dir: &str, types_dir: &str) {
        let imports = create_import_statements(self.borrow(), types_dir);
        let mut function_map: HashMap<String, Vec<String>> = HashMap::new();
        for function in self.functions.iter() {
            if !function_map.contains_key(&function.entity) {
                function_map.insert(function.entity.clone(), vec![function.function.clone()]);
            } else {
                function_map
                    .entry(function.entity.clone())
                    .and_modify(|e| e.push(function.function.clone()));
            }
        }
        let mut clients = String::from("");
        for entry in function_map.into_iter() {
            let client_name = entry.0;
            let functions: String = entry.1.join("\n");
            clients += &format!(
                "\n\nexport const {client_name}_client = {{\n{}\n}};\n",
                functions
            );
        }

        let final_string_to_write = format!(
            r#"//*********************************************************************************
//***THIS FILE IS GENERATED AUTOMATICALLY AND WILL BE OVERWRITTEN. DO NOT MODIFY***
//*********************************************************************************

{}

{}
            "#,
            imports, clients,
        );
        fs::write(
            format!("{client_dir}/generated_client.ts"),
            final_string_to_write,
        )
        .unwrap();
    }
}
fn is_bindings_file(path: &PathBuf) -> bool {
    let path_str = path.to_str().unwrap();
    path.is_file() && path_str.contains("bindings.ts") && !path_str.contains("/target/")
}
fn get_bindings(path: &PathBuf) -> Vec<String> {
    let content = fs::read_to_string(path).expect("to be able to read this file into a string");

    let re = RegexBuilder::new(r"export (interface|type) (?<name>\w+) (\{|=)")
        .build()
        .unwrap();

    re.captures_iter(&content)
        .filter_map(|e| e.name("name"))
        .map(|name| name.as_str().to_owned())
        .collect()
}

fn create_import_statements(output_file: &OutputFileContent, types_dir: &str) -> String {
    let mut imports = Vec::new();
    imports.push(format!(
        "import type {{{}}} from \"{types_dir}/bindings\";",
        output_file
            .clone()
            .bindings
            .into_iter()
            .collect::<HashSet<_>>()
            .into_iter()
            .collect::<Vec<String>>()
            .join(", ")
    ));
    imports.push(format!("export * from \"{types_dir}\";"));
    imports.push(format!("import {{ baseApiUrl, handleError }} from \".\""));

    imports.push(format!("import {{ Try, Err }} from \"@oxi\";"));

    imports.push(format!(
        r#"
export type ListOptions = {{
  limit?: number,
  offset?: number,
  order_bys?: string,
}};

export type DataRpcResult<T> = {{
    data: T
}};

export type RpcResult<T> = {{ id: string, jsonrpc: number, result: DataRpcResult<T> }};

export type ClientErrorValue = {{
  data: {{
    detail: ClientError["detail"]
    req_uuid: string
  }},
  message: ClientError["message"]
}};

export type RpcError = {{ id: string, jsonrpc: number, error: ClientErrorValue }};

export type ParamsIded = {{ id: string }};

export type ParamsForCreate<T> = {{ data: T }};

export type ParamsForUpdate<T> = {{ id: string, data: T }};

export type ParamsList<T> = {{
  filters?: Partial<Record<keyof T, any>>[],
  list_options?: {{
    limit?: number,
    offset?: number,
    order_bys?: string,
  }}
}};

const reqConfig: RequestInit = {{
        method: "POST",
        headers: {{
          "Content-Type": "application/json",
        }},
        credentials: "include",
      }};
"#
    ));

    imports.join("\n")
}

fn get_route_builder_fns(path: &PathBuf) -> Vec<String> {
    let mut content = fs::read_to_string(path).expect("to be able to read this file into a string");

    content = content
        .lines()
        .map(|l| {
            if l.trim().starts_with("//") {
                return "";
            } else {
                return l;
            }
        })
        .collect();

    let re_macro = Regex::new(r"router_builder!\(([\s\S]*?)\)").unwrap();

    if let Some(caps) = re_macro.captures(&content) {
        caps.get(1)
            .map_or("", |m| m.as_str())
            .split(",")
            .map(|s| s.trim().to_owned())
            .filter(|s| !s.is_empty())
            .collect()
    } else {
        vec![]
    }
}

fn get_common_rpc_fns(path: &PathBuf) -> Vec<CommonRpcFnsMacroItem> {
    let content = fs::read_to_string(path).expect("to be able to read this file into a string");

    let re_pairs = Regex::new(r"(?m)^\s+(?<name>\w+):\s+(?<entity>\w+),?$").unwrap();

    let re_macro = Regex::new(r"generate_common_rpc_fns!\(([\s\S]*?)\)").unwrap();

    let mut macro_items: Vec<CommonRpcFnsMacroItem> = vec![];

    if let Some(caps) = re_macro.captures(&content) {
        let content_inside = caps.get(1).map_or("", |m| m.as_str());

        for cap in re_pairs.captures_iter(content_inside) {
            macro_items.push(CommonRpcFnsMacroItem {
                name: String::from(&cap[1]),
                model_type: String::from(&cap[2]),
            });
        }
    }

    macro_items
}

#[derive(Debug)]
struct CommonRpcFnsMacroItem {
    name: String,
    model_type: String,
}

#[derive(Debug, Clone)]
struct EntityFunction {
    entity: String,
    function: String,
}

#[derive(Debug)]
struct HandlerParams {
    params: Vec<Param>,
    result: String,
}

#[derive(Debug)]
struct Param {
    name: String,
    _type: String,
}

fn process_rpc_file(path: &PathBuf, bindings: &Vec<String>) -> Result<Vec<EntityFunction>> {
    let file_name = path.file_name().unwrap();
    let entity = file_name
        .to_str()
        .to_owned()
        .unwrap()
        .replace("_rpc.rs", "");
    let mut handler_names = get_handler_names_manual(path);
    handler_names.append(&mut get_route_builder_fns(path));
    let mut functions = handler_names
        .iter()
        .map(|handler_name| {
            let handler_params: HandlerParams = get_handlers_params(path, handler_name);
            create_client_from_handler_params(handler_name, &handler_params, bindings)
        })
        .filter(|s| s != "")
        .collect::<Vec<String>>();

    functions.push(
        get_handlers_from_route_builder(path)?
            .iter()
            .map(|(handler_name, handler_params)| {
                create_client_from_handler_params(handler_name, handler_params, bindings)
            })
            .collect(),
    );
    if functions.len() == 0 {}
    let mut vec_of_entities: Vec<EntityFunction> = vec![];
    for function in functions {
        if function == "" {
            continue;
        }
        vec_of_entities.push(EntityFunction {
            entity: entity.clone(),
            function,
        });
    }

    Ok(vec_of_entities)
}

fn is_rpc_file(path: &PathBuf) -> bool {
    let path_str = path.to_str().unwrap();
    path.is_file()
        && path_str.contains("lib-rpc")
        && path_str.contains("_rpc.rs")
        && !path_str.contains("/target/")
}

fn type_exists_in_bindings(type_to_check: &str, bindings: &Vec<String>) -> bool {
    if type_to_check == "null" || type_to_check == "String" {
        return true;
    }
    for binding in bindings.iter() {
        if binding == type_to_check || type_to_check.contains(&format!("<{binding}>")) {
            return true;
        }
    }
    return false;
}

fn create_client_from_handler_params(
    handler_name: &str,
    handler_params: &HandlerParams,
    bindings: &Vec<String>,
) -> String {
    let first_param = &handler_params.params.get(0);

    let return_type = &handler_params.result;
    if first_param.is_none() {
        return "".to_owned();
    }

    let handler_param_type = &first_param.unwrap()._type;

    // let regex = Regex::new("(ParamsForCreate|ParamsList|ParamsForUpdate)<(?P<Entity>.*)>").unwrap();
    // let caps = regex.captures(&handler_param_type);
    // let mut client_param_type = {
    //     let mut s = String::from("");
    //     if let Some(caps) = caps {
    //         let thing = &caps.name("Entity");
    //
    //         if let Some(name) = thing {
    //             let name = name.as_str();
    //             s = name.to_owned();
    //         }
    //     }
    //     s
    // };
    //

    let client_param_type = handler_param_type
        .replace("Vec", "Array")
        .replace("i64", "string")
        .replace("()", "null");

    let mut client_param_name = "params".to_owned();

    // if client_param_type.starts_with("Vec<") {
    //     let vec_regex = Regex::new("Vec<(?P<entity>.*)>").unwrap();
    //     let caps = vec_regex.captures(&client_param_type);
    //
    //     if let Some(caps) = caps {
    //         let t = &caps.name("entity");
    //
    //         if let Some(name) = t {
    //             let name = name.as_str();
    //             let mut lower_case_name = camel_to_snake(name);
    //             // FIXME: this is not a sufficient check for good naming but itll do for now
    //             let length = lower_case_name.split("y_").collect::<Vec<&str>>().len();
    //             if length < 3 {
    //                 lower_case_name = lower_case_name.replace("y_", "ies_");
    //                 client_param_name = lower_case_name;
    //             } else {
    //                 client_param_name = lower_case_name;
    //             }
    //         }
    //     }
    // }

    let mut client_return_type = String::from("null");

    let vec_regex = Regex::new("<DataRpcResult<(?P<entity>.*)>>").unwrap();
    let caps = vec_regex.captures(&return_type);

    if let Some(caps) = caps {
        let t = &caps.name("entity");

        if let Some(name) = t {
            let return_type = name
                .as_str()
                .replace("Vec", "Array")
                .replace("i64", "string")
                .replace("()", "null");

            client_return_type = return_type
        }
    }

    let mut params_object = String::from("");

    if handler_param_type.contains("ParamsForCreate") {
        client_param_name = "params".to_owned();
        params_object += &format!("...params");
    };
    if handler_param_type.contains("ParamsForUpdate") {
        params_object += &format!("\"id\": params.id,");
        params_object += &format!("\n\t    \"data\": params.data,");
    };
    if handler_param_type.contains("ParamsList") {
        client_param_name = "params".to_owned();
        params_object += &format!("...params,");
    };
    if handler_param_type.contains("ParamsIded") {
        params_object += &format!("\"id\": params.id");
        client_param_name = "params".to_owned();
    }

    // if handler_param_type.contains("ListOptions") {
    //     params_object += &format!("\"list_options\": {client_param_name}");
    //     client_param_type = handler_param_type.to_owned();
    // }

    let colon = if !client_param_name.is_empty() {
        ": "
    } else {
        ""
    };

    // if handler_name == "list_patients" {
    //     println!("{handler_params:#?}");
    //     println!("client_param_name: {client_param_name}");
    //     println!("hadnler_param_type: {handler_param_type:#?}");
    //     println!("params: {params_object}")
    // }

    // FIXME: figure this out later
    // if client_param_type.contains("ParamsForUpdate<"){
    //     params_object += format!("\"id\": {client_param_name}");
    // }
    //
    let function = format!(
        r#"    async {handler_name}({client_param_name}{colon}{client_param_type}) {{
      const happyPath = async () => fetch(`${{baseApiUrl}}/api/rpc`, {{
        ...reqConfig,
        body: JSON.stringify({{
          id: 1,
          jsonrpc: "2.0",
          method: "{handler_name}",
          params: {{
            {params_object}
          }},
        }}),
      }}) as unknown as Promise<RpcResult<{client_return_type}>>;
      const val = await Try(happyPath, (e: RpcError) => Err(e));
      if (val.isError && handleError){{
          handleError(val);
      }}
      return val;
    }},
"#
    );

    // if !type_exists_in_bindings(&client_param_type, bindings)
    //     || !type_exists_in_bindings(&client_return_type, bindings)
    // {
    //     if handler_param_type.contains("ParamsList") {
    //         println!("ParamsList!");
    //     }
    //     println!("WARNING! Skipping handler: {handler_name}. Either {client_param_type} or {client_return_type} does not exist in bindings");
    //     return "".to_owned();
    // }
    //
    function
}
fn get_handler_names_manual(path: &PathBuf) -> Vec<String> {
    let content = fs::read_to_string(path).expect("to be able to read this file into a string");

    let re = RegexBuilder::new(r"(?<name>\w+)\s*\.into_dyn()")
        .build()
        .unwrap();

    re.captures_iter(&content)
        .filter_map(|e| e.name("name"))
        .map(|name| name.as_str().to_owned())
        .collect()
}

fn get_handlers_from_route_builder(path: &PathBuf) -> Result<Vec<(String, HandlerParams)>> {
    let common_rpc_fns = get_common_rpc_fns(path);

    let macro_items = get_route_builder_fns(path);

    if common_rpc_fns.len() == 0 {
        return Ok(vec![]);
    }

    let entity = &common_rpc_fns.iter().find(|f| &f.name == "Entity").ok_or(
        Error::EntityMissingFromRpcFns(path.to_str().unwrap().to_owned()),
    )?;

    let suffix = &common_rpc_fns
        .iter()
        .find(|f| &f.name == "Entity")
        .ok_or(Error::SuffixMissingFromRpcFns)?;

    let mut handlers: Vec<(String, HandlerParams)> = vec![];
    for handler_name in macro_items {
        let return_type =
            get_builder_item_return_type(&handler_name, &suffix.model_type, &entity.model_type);

        if let Err(Error::CantMatchHandlerReturnType(e)) = return_type {
            println!("WARNING: Handler: {handler_name}, Error: Cant match handler return type. Ignoring as it might be a function defined outside of the generate_common_rpc_fns macro.");
            continue;
        } else {
            let return_type = return_type.unwrap();
            let params =
                get_builder_item_params(&handler_name, &suffix.model_type, &common_rpc_fns)?;

            handlers.push((
                handler_name,
                HandlerParams {
                    params: vec![Param {
                        name: "params".to_owned(),
                        _type: params,
                    }],
                    result: return_type,
                },
            ));
        }
    }

    Ok(handlers)
}

fn get_builder_item_return_type(
    handler_name: &str,
    capital_suffix: &str,
    entity: &str,
) -> Result<String> {
    let suffix = camel_to_snake(capital_suffix);
    let get_handler = format!("get_{}", suffix);
    let create_handler = format!("create_{}", suffix);
    let delete_handler = format!("delete_{}", suffix);
    let update_handler = format!("update_{}", suffix);
    let list_handler = format!("list_{}s", suffix);

    if handler_name == get_handler
        || handler_name == create_handler
        || handler_name == delete_handler
        || handler_name == update_handler
    {
        Ok(format!("Result<DataRpcResult<{}>>", entity))
    } else if handler_name == list_handler {
        Ok(format!("Result<DataRpcResult<Vec<{}>>>", entity))
    } else {
        Err(Error::CantMatchHandlerReturnType(handler_name.to_owned()))
    }
}

fn get_builder_item_params(
    handler_name: &str,
    capital_suffix: &str,
    common_rpc_macro_items: &Vec<CommonRpcFnsMacroItem>,
) -> Result<String> {
    let suffix = camel_to_snake(capital_suffix);
    let get_handler = format!("get_{}", suffix);
    let create_handler = format!("create_{}", suffix);
    let delete_handler = format!("delete_{}", suffix);
    let update_handler = format!("update_{}", suffix);
    let list_handler = format!("list_{}s", suffix);

    if handler_name == get_handler || handler_name == delete_handler {
        Ok("ParamsIded".to_string())
    } else if handler_name == create_handler {
        let fc = common_rpc_macro_items
            .iter()
            .find(|f| &f.name == "ForCreate")
            .ok_or(Error::ForCreateMissingFromRpcFns)?;
        Ok(format!("ParamsForCreate<{}>", fc.model_type))
    } else if handler_name == update_handler {
        let fu = common_rpc_macro_items
            .iter()
            .find(|f| &f.name == "ForUpdate")
            .ok_or(Error::ForUpdateMissingFromRpcFns)?;
        Ok(format!("ParamsForUpdate<{}>", fu.model_type))
    } else if handler_name == list_handler {
        let filter = common_rpc_macro_items
            .iter()
            .find(|f| &f.name == "Filter")
            .ok_or(Error::FilterMissingFromRpcFns)?;
        Ok(format!("ParamsList<{}>", filter.model_type))
    } else {
        Err(Error::CantMatchHandlerParams(handler_name.to_owned()))
    }
}

fn get_handlers_params(path: &PathBuf, handler_name: &str) -> HandlerParams {
    let content = fs::read_to_string(path).expect("to be able to read this file into a string");

    let pattern_string = format!(
        r"(?s)async fn {}\((?P<params>.*?)\)\s*->\s(?P<result>.*?)\s*\{{",
        regex::escape(&handler_name)
    );

    let re = RegexBuilder::new(&pattern_string).build().unwrap();
    let ctx_mm_remove_regex = Regex::new(r",?\s*(ctx: Ctx|mm: ModelManager)\s*,?").unwrap();
    let params = re
        .captures_iter(&content)
        .filter_map(|e| e.name("params"))
        .map(|name| {
            let name = name.as_str().to_owned().trim().to_owned();
            ctx_mm_remove_regex
                .replace_all(&name, "")
                .trim()
                .to_string()
        })
        .flat_map(|s| {
            s.split(",")
                .filter(|s| *s != "")
                .map(|s| {
                    let param_type = s.split(": ").map(|s| s.to_owned()).collect::<Vec<String>>();
                    Param {
                        name: param_type[0].clone(),
                        _type: param_type[1].clone(),
                    }
                })
                .collect::<Vec<Param>>()
        })
        .collect::<Vec<Param>>();

    // .flat_map(|s| s.split(",").collect::<Vec<String>>())
    // .collect::<Vec<Vec<String>>>();
    let result = re
        .captures_iter(&content)
        .filter_map(|e| e.name("result"))
        .map(|name| name.as_str().to_owned())
        .collect::<String>();

    HandlerParams { params, result }
}
