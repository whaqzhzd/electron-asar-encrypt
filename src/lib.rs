#![deny(clippy::all)]
use encrypt::electron_run_as_node_vaild;
use encrypt::show_error_and_quit;
use encrypt::NativeObject;
use napi::Env;
use napi::Error;
use napi::JsFunction;
use napi::JsObject;
use napi::JsString;
use napi::JsUnknown;
use napi::Property;
use napi_derive::module_exports;

mod encrypt;

#[module_exports]
fn mod_init(exports: JsObject, env: Env) -> Result<(), napi::Error> {
  let global = env.get_global()?;

  #[cfg(feature = "dev")]
  dbg!("mod_init");

  let mut is_renderer: bool = false;
  let main_module = {
    if env
      .get_global()
      .unwrap()
      .has_named_property("module")
      .unwrap()
    {
      // electron renderer
      is_renderer = true;
      global.get_named_property::<JsObject>("module").unwrap()
    } else {
      let process: JsObject = global.get_named_property("process").unwrap();
      let argv: JsObject = process.get_named_property("argv").unwrap();
      let leng = argv.get_array_length().unwrap();

      for x in 0..leng {
        let arg: JsString = argv.get_element::<JsString>(x).unwrap();
        if arg.into_utf8()?.as_str()?.contains("--inspect")
          || arg
            .into_utf8()?
            .as_str()?
            .contains("--remote-debugging-port")
        {
          return Err(Error::new(
            napi::Status::InvalidArg,
            "Not allow debugging this program.".to_string(),
          ));
        }
      }

      process
        .get_named_property::<JsObject>("mainModule")
        .unwrap()
    }
  };

  #[cfg(feature = "dev")]
  dbg!(is_renderer);

  let this_module = encrypt::get_module_object(&env, &main_module, &exports);
  let require = encrypt::make_require_function(&env, &this_module);

  let module_constructor = require
    .call(None, &[&env.create_string("module").unwrap()])
    .unwrap()
    .coerce_to_object()
    .unwrap();

  let electron = require
    .call(None, &[&env.create_string("electron").unwrap()])
    .unwrap()
    .coerce_to_object()
    .unwrap();

  #[cfg(feature = "dev")]
  dbg!("准备判断");

  if is_renderer {
    let eq = !env
      .strict_equals(
        this_module
          .get_named_property::<JsObject>("parent")
          .unwrap(),
        main_module,
      )
      .unwrap();

    #[cfg(feature = "dev")]
    dbg!("module_parent != main_module", eq);

    if eq {
      let ipc_renderer = electron
        .get_named_property::<JsObject>("ipcRenderer")
        .unwrap();
      let send_sync = ipc_renderer
        .get_named_property::<JsFunction>("sendSync")
        .unwrap();

      send_sync
        .call(
          Some(&ipc_renderer),
          &[env.create_string("__SHOW_ERROR_AND_QUIT__").unwrap()],
        )
        .unwrap();

      return Ok(());
    }
  } else {
    let equals_this_main: bool = env
      .strict_equals(
        encrypt::get_module_object(&env, &main_module, &exports),
        main_module,
      )
      .unwrap();

    let equals_module_parent_module: bool = env
      .strict_equals(
        this_module
          .get_named_property::<JsObject>("parent")
          .unwrap(),
        require
          .call(None, &[&env.create_string("module").unwrap()])
          .unwrap()
          .coerce_to_object()
          .unwrap(),
      )
      .unwrap();

    let equals_module_parent_not_undefined: bool = env
      .strict_equals(
        this_module
          .get_named_property::<JsObject>("parent")
          .unwrap(),
        env.get_undefined().unwrap(),
      )
      .unwrap();
    let equals_module_parent_not_null: bool = env
      .strict_equals(
        this_module
          .get_named_property::<JsObject>("parent")
          .unwrap(),
        env.get_null().unwrap(),
      )
      .unwrap();

    // 调试的时候equals_module_parent_module为true
    // 发布的时候equals_module_parent_module为false
    if !equals_this_main
      || (!equals_module_parent_module
        && !equals_module_parent_not_undefined
        && !equals_module_parent_not_null)
    {
      #[cfg(feature = "dev")]
      dbg!(
        equals_this_main,
        equals_module_parent_module,
        equals_module_parent_not_undefined,
        equals_module_parent_not_null
      );

      show_error_and_quit(&env, &electron, &env.create_string("not allow").unwrap());
      return Ok(());
    }
  }

  let mut module_prototype: JsObject = module_constructor.get_named_property("prototype").unwrap();
  let prototype_compile: JsFunction = module_prototype.get_named_property("_compile").unwrap();
  let addon_data = env
    .get_instance_data::<NativeObject>()
    .unwrap()
    .unwrap_or_else(|| {
      env
        .set_instance_data(NativeObject::new(), 0, |_ctx| {})
        .unwrap();
      env.get_instance_data::<NativeObject>().unwrap().unwrap()
    });

  #[cfg(feature = "dev")]
  dbg!("添加原生编译函数");

  addon_data.functions.insert(0, prototype_compile);
  let _ = module_prototype.define_properties(&[
    Property::new("_compile")?.with_method(encrypt::module_prototype_compile)
  ]);

  if is_renderer {
    // 客户端有systemjs
    if env
      .get_global()
      .unwrap()
      .has_named_property("System")
      .unwrap()
    {
      let s = env
        .get_global()
        .unwrap()
        .get_named_property::<JsObject>("System")
        .unwrap();

      let has_func = s.has_named_property("createScript").unwrap();
      if has_func {
        let system_create_script = s.get_named_property::<JsFunction>("createScript").unwrap();
        addon_data.functions.insert(1, system_create_script);

        #[cfg(feature = "dev")]
        dbg!("添加原生SystemJS编译函数");

        let _ = s
          .get_named_property::<JsObject>("__proto__")
          .unwrap()
          .define_properties(&[
            Property::new("createScript")?.with_method(encrypt::systemjs_create_scripts)
          ]);
      }
    };

    let _ = env.get_global().unwrap().define_properties(&[
      Property::new("downloadScript")?.with_method(encrypt::ccjs_download_scripts)
    ]);

    env
      .run_script::<_, JsUnknown>(
        &"function create_script(content){
        var s = document.createElement(\"script\");
        const blob = new Blob([content], {type: \"application/javascript\"}); 
        const url = URL.createObjectURL(blob);
        s.src = url; 
        s.addEventListener('load', function () {
          URL.revokeObjectURL(url);
        });
        return s;
      };",
      )
      .unwrap();

    #[cfg(feature = "dev")]
    dbg!("renderer已准备好");

    return Ok(());
  };

  let vaild = electron_run_as_node_vaild(&env);

  if vaild.0 || vaild.1 || vaild.2 {
    #[cfg(feature = "dev")]
    dbg!("electron_run_as_node_vaild", vaild);

    let ipc_main = electron.get_named_property::<JsObject>("ipcMain").unwrap();
    let once = ipc_main.get_named_property::<JsFunction>("once").unwrap();

    once
      .call(
        Some(&ipc_main),
        &[
          env
            .create_string("__SHOW_ERROR_AND_QUIT__")
            .unwrap()
            .into_unknown(),
          env
            .create_function_from_closure("showErrorAndQuit", move |ctx| {
              let mut event = ctx.get::<JsObject>(0).unwrap();
              let mm = ctx
                .env
                .get_global()
                .unwrap()
                .get_named_property::<JsObject>("process")
                .unwrap()
                .get_named_property::<JsObject>("mainModule")
                .unwrap();
              let req = mm.get_named_property::<JsFunction>("require").unwrap();
              show_error_and_quit(
                &env,
                &req
                  .call(Some(&mm), &[env.create_string("electron").unwrap()])
                  .unwrap()
                  .coerce_to_object()
                  .unwrap(),
                &env.create_string("0").unwrap(),
              );
              event.set("returnValue", env.get_null().unwrap()).unwrap();
              Ok(())
            })
            .unwrap()
            .into_unknown(),
        ],
      )
      .unwrap();
  };

  let main = unsafe {
    require
      .call(None, &[&env.create_string("./index.js").unwrap()])
      .unwrap()
      .cast::<JsFunction>()
  };

  //key_array(&env)
  main
    .call::<JsUnknown>(None, &[])
    .map_err(|e| {
      show_error_and_quit(
        &env,
        &electron,
        &env.create_string(e.reason.as_str()).unwrap(),
      );
    })
    .unwrap();

  #[cfg(feature = "dev")]
  dbg!("main已准备好");

  Ok(())
}
