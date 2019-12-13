use std::cell::Ref;
use wasmtime::{Engine, HostRef, Instance, Limits, Module, Store};

use wasmtime_environ::{entity::PrimaryMap, wasm::DefinedMemoryIndex, MemoryPlan, Tunables};
use wasmtime_runtime::{LinearMemory, Mmap};

#[test]
fn test_external_memory() {
    const WAT: &str = r#"
    (module
      (func $run (export "run") (i32.store (i32.const 0) (i32.const 123)))
      (memory (export "memory") 1 4)
    )
    "#;

    let engine = HostRef::new(Engine::default());
    let store = HostRef::new(Store::new(&engine));
    let wasm = wat::parse_str(WAT).unwrap();
    let module = HostRef::new(Module::new(&store, &wasm).expect("failed to create module"));

    // memories allocated by the host and given to the runtime
    let (minimum, maximum) = {
        let module = module.borrow();
        let limits: &Limits = module.exports()[1].ty().memory().unwrap().limits();
        println!("{:?}", limits);
        (limits.min(), limits.max())
    };

    let plan = {
        let memory = wasmtime_environ::wasm::Memory {
            minimum,
            maximum,
            shared: false,
        };
        MemoryPlan::for_memory(memory, &Tunables::default())
    };

    let mut memories: PrimaryMap<DefinedMemoryIndex, LinearMemory> = PrimaryMap::new();
    memories.push(
        // .new_with_external_memory() does some additional checks and calculations (like taking
        // into consideration guard pages). We feed those to a maker function...
        LinearMemory::new_with_external_memory(&plan, &mut |accessible_memory, total_memory| {
            Mmap::accessible_reserved(accessible_memory, total_memory)
        })
        .unwrap(),
    );
    for (ix, mem) in memories.iter() {
        println!(
            "external memory {}: {:?}",
            DefinedMemoryIndex::as_u32(ix),
            mem
        );
    }

    let mem = memories.get(DefinedMemoryIndex::from_u32(0u32)).unwrap();
    let mem_base: *const u8 = mem.mmap.as_ptr();

    let instance = HostRef::new(
        Instance::new_with_external_memory(&store, &module, &[], memories.into_boxed_slice())
            .expect("failed to instantiate module"),
    );

    let exports = Ref::map(instance.borrow(), |instance| instance.exports());
    assert!(!exports.is_empty());

    let run_func = exports[0]
        .func()
        .expect("expected a run func in the module");

    run_func
        .borrow()
        .call(&[])
        .expect("expected function not to trap");

    let value = unsafe { std::ptr::read::<i32>(mem_base as *mut i32) };
    assert_eq!(value, 123);
}
