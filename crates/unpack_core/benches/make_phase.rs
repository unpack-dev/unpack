use std::{
    fs, io,
    path::{Path, PathBuf},
};

use tokio::runtime::Runtime;
use unpack_core::{Compiler, CompilerOptions, Entry};

fn main() {
    divan::main();
}

#[divan::bench]
fn make_phase_small(bencher: divan::Bencher) {
    bench_make_phase(
        bencher,
        FixtureShape {
            name: "small",
            feature_modules: 10,
            shared_modules: 5,
            imports_per_feature: 3,
        },
    );
}

#[divan::bench]
fn make_phase_medium(bencher: divan::Bencher) {
    bench_make_phase(
        bencher,
        FixtureShape {
            name: "medium",
            feature_modules: 100,
            shared_modules: 20,
            imports_per_feature: 4,
        },
    );
}

#[divan::bench]
fn make_phase_large(bencher: divan::Bencher) {
    bench_make_phase(
        bencher,
        FixtureShape {
            name: "large",
            feature_modules: 500,
            shared_modules: 50,
            imports_per_feature: 5,
        },
    );
}

fn bench_make_phase(bencher: divan::Bencher, shape: FixtureShape) {
    let fixture = Fixture::generate(shape).expect("make phase benchmark fixture should be created");
    let runtime = Runtime::new().expect("benchmark runtime should be created");
    let compiler = Compiler::new(CompilerOptions::new(
        fixture.context.clone(),
        vec![Entry::new("main", "./src/index")],
    ));

    bencher.bench_local(|| {
        let compilation = runtime
            .block_on(compiler.run())
            .expect("make phase benchmark fixture should compile");

        divan::black_box((
            compilation.entries().len(),
            compilation.module_graph().modules().len(),
            compilation.module_graph().connections().len(),
        ));
    });
}

#[derive(Debug, Clone, Copy)]
struct FixtureShape {
    name: &'static str,
    feature_modules: usize,
    shared_modules: usize,
    imports_per_feature: usize,
}

struct Fixture {
    _temp: tempfile::TempDir,
    context: PathBuf,
}

impl Fixture {
    fn generate(shape: FixtureShape) -> io::Result<Self> {
        let temp = tempfile::Builder::new().prefix(shape.name).tempdir()?;
        let context = temp.path().to_path_buf();

        write_file(&context.join("src/index.js"), &entry_source(shape))?;

        for shared in 0..shape.shared_modules {
            write_file(
                &context.join(format!("src/shared/shared{shared}.js")),
                &shared_source(shared),
            )?;
        }

        for feature in 0..shape.feature_modules {
            write_file(
                &context.join(format!("src/features/feature{feature}.js")),
                &feature_source(shape, feature),
            )?;
            write_file(
                &context.join(format!("src/features/leaf{feature}.js")),
                &leaf_source(feature),
            )?;
            write_file(
                &context.join(format!("src/features/reexport{feature}.js")),
                &reexport_source(feature),
            )?;
        }

        Ok(Self {
            _temp: temp,
            context,
        })
    }
}

fn entry_source(shape: FixtureShape) -> String {
    let mut source = String::new();
    for feature in 0..shape.feature_modules {
        source.push_str(&format!(
            "import {{ feature{feature} }} from \"./features/feature{feature}\";\n"
        ));
    }

    source.push_str("export const entry = [\n");
    for feature in 0..shape.feature_modules {
        source.push_str(&format!("  feature{feature},\n"));
    }
    source.push_str("];\n");
    source
}

fn feature_source(shape: FixtureShape, feature: usize) -> String {
    let mut source = String::new();
    let shared_imports = shared_imports(shape, feature);

    for shared in &shared_imports {
        source.push_str(&format!(
            "import {{ shared{shared} }} from \"../shared/shared{shared}\";\n"
        ));
    }

    source.push_str(&format!(
        "import {{ leaf{feature} }} from \"./leaf{feature}\";\n"
    ));
    source.push_str(&format!(
        "export {{ reexport{feature} }} from \"./reexport{feature}\";\n"
    ));
    source.push_str(&format!(
        "export * from \"../shared/shared{}\";\n",
        feature % shape.shared_modules
    ));
    source.push_str(&format!("export const feature{feature} = leaf{feature}"));
    for shared in shared_imports {
        source.push_str(&format!(" + shared{shared}"));
    }
    source.push_str(";\n");
    source
}

fn shared_imports(shape: FixtureShape, feature: usize) -> Vec<usize> {
    (0..shape.imports_per_feature)
        .map(|offset| (feature + offset) % shape.shared_modules)
        .collect()
}

fn shared_source(shared: usize) -> String {
    format!(
        "export const shared{shared} = {shared};\nexport const sharedValue{shared} = shared{shared} * 2;\n"
    )
}

fn leaf_source(feature: usize) -> String {
    format!("export const leaf{feature} = {feature};\n")
}

fn reexport_source(feature: usize) -> String {
    format!("export {{ leaf{feature} as reexport{feature} }} from \"./leaf{feature}\";\n")
}

fn write_file(path: &Path, source: &str) -> io::Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(path, source)
}
