use base64::{engine::general_purpose, Engine as _};
use napi::{CallContext, Env, JsFunction, JsObject, JsString};
use napi::{JsArrayBuffer, JsUnknown, ValueType};
use napi_derive::js_function;
use std::path::Path;
use std::{collections::HashMap, process::exit};

use aes::Aes256;
use block_modes::block_padding::Pkcs7;
use block_modes::{BlockMode, Cbc};

type AesCbc = Cbc<Aes256, Pkcs7>;

pub struct NativeObject {
  pub functions: HashMap<i32, JsFunction>,
}

impl NativeObject {
  pub fn new() -> NativeObject {
    Self {
      functions: HashMap::new(),
    }
  }
}

fn decode_uri(url: &JsString, ctx: &CallContext) -> JsString {
  let req = ctx
    .env
    .get_global()
    .unwrap()
    .get_named_property::<JsFunction>("decodeURI")
    .unwrap();

  return req
    .call(Some(&ctx.this().unwrap()), &[url])
    .unwrap()
    .coerce_to_string()
    .unwrap();
}

fn read_file_sync(ctx: &CallContext) -> JsFunction {
  let req = ctx
    .env
    .get_global()
    .unwrap()
    .get_named_property::<JsFunction>("require")
    .unwrap();

  let fs = req
    .call(
      Some(&ctx.this().unwrap()),
      &[ctx.env.create_string("fs").unwrap()],
    )
    .unwrap()
    .coerce_to_object()
    .unwrap();
  fs.get_named_property::<JsFunction>("readFileSync").unwrap()
}

pub fn electron_run_as_node_vaild(env: &Env) -> (bool, bool, bool) {
  let electron_run_as_node = || {
    env
      .get_global()
      .unwrap()
      .get_named_property::<JsObject>("process")
      .unwrap()
      .get_named_property::<JsObject>("env")
      .unwrap()
      .get_named_property::<JsUnknown>("ELECTRON_RUN_AS_NODE")
      .unwrap()
  };

  let mut eq_zero = false;
  let mut eq_empty_string = false;
  if electron_run_as_node().get_type().unwrap() == ValueType::Number {
    eq_zero = electron_run_as_node()
      .coerce_to_number()
      .unwrap()
      .get_uint32()
      .unwrap()
      == 0;
  }

  if electron_run_as_node().get_type().unwrap() == ValueType::String {
    eq_empty_string = electron_run_as_node()
      .coerce_to_string()
      .unwrap()
      .into_utf8()
      .unwrap()
      .as_str()
      .unwrap()
      .is_empty();
  }

  let is_undefined = env
    .strict_equals(electron_run_as_node(), env.get_undefined().unwrap())
    .unwrap();

  (is_undefined, eq_zero, eq_empty_string)
}

pub fn show_error_and_quit(env: &Env, electron: &JsObject, message: &JsString) {
  let vaild = electron_run_as_node_vaild(env);
  if vaild.0 && vaild.1 && vaild.2 {
    println!("{:?}", message.into_utf8().unwrap().as_str());
    exit(1);
  } else {
    let dialog = electron.get_named_property::<JsObject>("dialog").unwrap();
    dialog
      .get_named_property::<JsFunction>("showErrorBox")
      .unwrap()
      .call(Some(&dialog), &[message])
      .unwrap();

    let app = electron.get_named_property::<JsObject>("app").unwrap();
    let quit = app.get_named_property::<JsFunction>("quit").unwrap();
    quit.call::<JsUnknown>(Some(&app), &[]).unwrap();
  }
}

pub fn get_module_object(env: &Env, main_module: &JsObject, this_exports: &JsObject) -> JsObject {
  let find = env
    .get_global()
    .unwrap()
    .has_named_property("findEntryModule")
    .unwrap();

  if find {
    return env
      .get_global()
      .unwrap()
      .get_named_property::<JsFunction>("findEntryModule")
      .unwrap()
      .call(None, &[main_module, this_exports])
      .unwrap()
      .coerce_to_object()
      .unwrap();
  }

  let _: JsUnknown = env
    .run_script(
      "function findEntryModule(mainModule, exports) {
            function findModule(start, target) {
              if (start.exports === target) {
                return start;
              }
              for (var i = 0; i < start.children.length; i++) {
                var res = findModule(start.children[i], target);
                if (res) {
                  return res;
                }
              }
              return null;
            }
            return findModule(mainModule, exports);
          }",
    )
    .unwrap();

  let fun = env
    .get_global()
    .unwrap()
    .get_named_property::<JsFunction>("findEntryModule")
    .unwrap();

  fun
    .call(None, &[main_module, this_exports])
    .unwrap()
    .coerce_to_object()
    .unwrap()
}

pub fn make_require_function(env: &Env, mods: &JsObject) -> JsFunction {
  let _: JsUnknown =env
        .run_script(
            "function makeRequireFunction(mod) {
              const Module = mod.constructor;
            
              function validateString (value, name) { if (typeof value !== 'string') throw new TypeError('The \"' + name + '\" argument must be of type string. Received type ' + typeof value); }
            
              const require = function require(path) {
                return mod.require(path);
              };
            
              function resolve(request, options) {
                validateString(request, 'request');
                return Module._resolveFilename(request, mod, false, options);
              }
            
              require.resolve = resolve;
            
              function paths(request) {
                validateString(request, 'request');
                return Module._resolveLookupPaths(request, mod);
              }
            
              resolve.paths = paths;
            
              require.main = process.mainModule;
            
              require.extensions = Module._extensions;
            
              require.cache = Module._cache;
            
              return require;
            }",
        )
        .unwrap();

  unsafe {
    env
      .get_global()
      .unwrap()
      .get_named_property::<JsFunction>("makeRequireFunction")
      .unwrap()
      .call(None, &[mods])
      .unwrap()
      .cast::<JsFunction>()
  }
}

pub fn decrypt(env: &Env, base64: &str) -> JsString {
  let content = decrypt_string(env, base64);
  env.create_string_from_std(content).unwrap()
}

pub fn decrypt_string(_: &Env, base64: &str) -> String {
  let buf = general_purpose::STANDARD.decode(base64);
  if buf.is_err() {
    return base64.to_string();
  }
  let buf = buf.unwrap();
  // 前 16 字节是 IV
  // 16 字节以后是加密后的代码
  let iv = &buf[0..16];
  let data = &buf[16..];
  let key = include_str!("key.txt");
  let key: Vec<u8> = general_purpose::STANDARD.decode(key).unwrap();

  let cipher = AesCbc::new_from_slices(key.as_slice(), iv).unwrap();

  String::from_utf8(cipher.decrypt_vec(data).unwrap()).unwrap()
}

#[allow(dead_code)]
pub fn key_array(env: &Env) -> JsArrayBuffer {
  let key = include_str!("key.txt");
  let key: Vec<u8> = general_purpose::STANDARD.decode(key).unwrap();

  env.create_arraybuffer_with_data(key).unwrap().into_raw()
}

#[js_function(2)]
pub fn module_prototype_compile(ctx: CallContext) -> napi::Result<napi::JsUnknown> {
  let addon_data = ctx
    .env
    .get_instance_data::<NativeObject>()
    .unwrap()
    .unwrap();

  let content: JsString = ctx.get(0).unwrap();
  let filename: JsString = ctx.get(1).unwrap();

  let filename_str = filename.into_utf8().unwrap();
  let old_compile: &JsFunction = addon_data.functions.get(&0).unwrap();

  // 不在node_modules且后缀为js类型
  if !filename_str.as_str().unwrap().contains("node_modules")
    && filename_str.as_str().unwrap().ends_with(".js")
  {
    #[cfg(feature = "dev")]
    dbg!("主进程开始解密:", filename_str.as_str().unwrap());

    return old_compile.call(
      Some(&ctx.this().unwrap()),
      &[
        decrypt(ctx.env, content.into_utf8().unwrap().as_str().unwrap()),
        filename,
      ],
    );
  }

  old_compile.call(Some(&ctx.this().unwrap()), &[content, filename])
}

#[js_function(3)]
pub fn ccjs_download_scripts(ctx: CallContext) -> napi::Result<napi::JsUnknown> {
  let mut url: JsString = ctx.get(0).unwrap();
  let _: JsObject = ctx.get(1).unwrap();
  let on_complete: JsFunction = ctx.get(2).unwrap();

  url = decode_uri(&url, &ctx);

  let s = url.into_utf8().unwrap().as_str().unwrap().to_string();
  // 不在node_modules且后缀为js类型
  let encrypt = !s.contains("node_modules") && s.ends_with(".js");
  let read_file_sync = read_file_sync(&ctx);

  let dirname = ctx
    .env
    .get_global()
    .unwrap()
    .get_named_property::<JsString>("__dirname")
    .unwrap();

  let path = Path::new(dirname.into_utf8().unwrap().as_str().unwrap()).join(Path::new(&s));
  let path = path.to_str().unwrap();

  let content = read_file_sync
    .call(None, &[ctx.env.create_string(path).unwrap()])
    .unwrap()
    .coerce_to_string()
    .unwrap();
  let content = if encrypt {
    #[cfg(feature = "dev")]
    dbg!("渲染进程cc开始解密:", &path);

    let content = decrypt(ctx.env, content.into_utf8().unwrap().as_str().unwrap());
    content
  } else {
    content
  };

  let _: JsUnknown = ctx
    .env
    .run_script(content.into_utf8().unwrap().as_str().unwrap())
    .unwrap();

  on_complete
    .call(None, &[ctx.env.get_null().unwrap()])
    .unwrap();

  Ok(ctx.env.get_undefined().unwrap().into_unknown())
}

#[js_function(1)]
pub fn systemjs_create_scripts(ctx: CallContext) -> napi::Result<napi::JsUnknown> {
  let addon_data = ctx
    .env
    .get_instance_data::<NativeObject>()
    .unwrap()
    .unwrap();

  let mut url: JsString = ctx.get(0).unwrap();
  url = decode_uri(&url, &ctx);

  let old_create_scripts: &JsFunction = addon_data.functions.get(&1).unwrap();
  let read_file_sync = read_file_sync(&ctx);
  let s = url.into_utf8().unwrap().as_str().unwrap().to_string();
  // 不在node_modules且后缀为js类型
  let encrypt = !s.contains("node_modules") && s.ends_with(".js");

  if url
    .into_utf8()
    .unwrap()
    .as_str()
    .unwrap()
    .starts_with("file:///")
  {
    let n_url = &s["file:///".len()..];

    let content = read_file_sync
      .call(None, &[ctx.env.create_string(n_url).unwrap()])
      .unwrap()
      .coerce_to_string()
      .unwrap();
    let content = if encrypt {
      #[cfg(feature = "dev")]
      dbg!(
        "渲染进程开始解密:",
        url.into_utf8().unwrap().as_str().unwrap()
      );

      let content = decrypt(ctx.env, content.into_utf8().unwrap().as_str().unwrap());
      content
    } else {
      content
    };

    let scripts = ctx
      .env
      .get_global()
      .unwrap()
      .get_named_property::<JsFunction>("create_script")
      .unwrap();

    let obj = scripts
      .call::<JsString>(None, &[content])
      .unwrap()
      .coerce_to_object()
      .unwrap();

    Ok(obj.into_unknown())
  } else {
    old_create_scripts.call(Some(&ctx.this().unwrap()), &[url])
  }
}
