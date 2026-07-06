import { mkdir, rm, writeFile } from "node:fs/promises";
import { dirname, join } from "node:path";

export const WARM_BUILD_CHECKSUM_DELTA = 2000;

const WEBPACK_ALL_COMMIT = "d3a1ca290b4b887757a45901288ea30f86b2f842";
const LARGE_BASE_CHECKSUM = 100000;
const LOADER_MODULE_COUNT = 300;
const THREE_COPY_COUNT = 10;
const THREE_PARTS_PER_COPY = 20;
const ROME_MODULE_COUNT = 80;

export const FIXTURE_SHAPES = {
  large: {
    name: "large",
    kind: "webpack-all",
    expectedChecksum: LARGE_BASE_CHECKSUM
  },
  loader: {
    name: "loader",
    kind: "loader",
    moduleCount: LOADER_MODULE_COUNT,
    expectedChecksum: LARGE_BASE_CHECKSUM + loaderExpectedChecksum(LOADER_MODULE_COUNT)
  }
};

const WEBPACK_ALL_DEPENDENCIES = {
  "@atlaskit/editor-core": "^120.1.0",
  "@atlaskit/media-core": "^31.1.0",
  "@atlaskit/smart-card": "^13.0.0",
  "@babel/runtime": "^7.12.13",
  "@material-ui/core": "^4.11.3",
  "@material-ui/icons": "^4.11.2",
  "@material-ui/lab": "^4.0.0-alpha.57",
  acorn: "^8.0.5",
  assert: "^2.0.0",
  classnames: "^2.2.6",
  "date-fns": "^2.17.0",
  jquery: "^3.5.1",
  lodash: "^4.17.20",
  "lodash-es": "^4.17.20",
  moment: "^2.29.1",
  react: "^17.0.1",
  "react-dom": "^17.0.1",
  "react-intl": "^2.6.0",
  redux: "^4.0.5",
  rxjs: "^5.5.0",
  underscore: "^1.12.0",
  uuid: "^8.3.2",
  vue: "^2.6.12",
  "zone.js": "^0.11.3"
};

const WEBPACK_ALL_PACKAGES = [
  "@atlaskit/editor-core",
  "@atlaskit/media-core",
  "@atlaskit/smart-card",
  "@material-ui/core",
  "@material-ui/icons",
  "@material-ui/lab",
  "acorn",
  "assert",
  "classnames",
  "date-fns",
  "jquery",
  "lodash",
  "lodash-es",
  "moment",
  "react",
  "react-dom",
  "react-intl",
  "redux",
  "rxjs",
  "underscore",
  "uuid",
  "vue",
  "zone.js"
];

const BABEL_RUNTIME_HELPERS = [
  "typeof",
  "jsx",
  "asyncIterator",
  "AwaitValue",
  "AsyncGenerator",
  "wrapAsyncGenerator",
  "awaitAsyncGenerator",
  "asyncGeneratorDelegate",
  "asyncToGenerator",
  "classCallCheck",
  "createClass",
  "defineEnumerableProperties",
  "defaults",
  "defineProperty",
  "extends",
  "objectSpread",
  "objectSpread2",
  "inherits",
  "inheritsLoose",
  "getPrototypeOf",
  "setPrototypeOf",
  "isNativeReflectConstruct",
  "construct",
  "isNativeFunction",
  "wrapNativeSuper",
  "instanceof",
  "interopRequireDefault",
  "interopRequireWildcard",
  "newArrowCheck",
  "objectDestructuringEmpty",
  "objectWithoutPropertiesLoose",
  "objectWithoutProperties",
  "assertThisInitialized",
  "possibleConstructorReturn",
  "createSuper",
  "superPropBase",
  "get",
  "set",
  "taggedTemplateLiteral",
  "taggedTemplateLiteralLoose",
  "readOnlyError",
  "writeOnlyError",
  "classNameTDZError",
  "temporalUndefined",
  "tdz",
  "temporalRef",
  "slicedToArray",
  "slicedToArrayLoose",
  "toArray",
  "toConsumableArray",
  "arrayWithoutHoles",
  "arrayWithHoles",
  "maybeArrayLike",
  "iterableToArray",
  "iterableToArrayLimit",
  "iterableToArrayLimitLoose",
  "unsupportedIterableToArray",
  "arrayLikeToArray",
  "nonIterableSpread",
  "nonIterableRest",
  "createForOfIteratorHelper",
  "createForOfIteratorHelperLoose",
  "skipFirstGeneratorNext",
  "toPrimitive",
  "toPropertyKey",
  "initializerWarningHelper",
  "initializerDefineProperty",
  "applyDecoratedDescriptor",
  "classPrivateFieldLooseKey",
  "classPrivateFieldLooseBase",
  "classPrivateFieldGet",
  "classPrivateFieldSet",
  "classPrivateFieldDestructureSet",
  "classStaticPrivateFieldSpecGet",
  "classStaticPrivateFieldSpecSet",
  "classStaticPrivateMethodGet",
  "classStaticPrivateMethodSet",
  "decorate",
  "classPrivateMethodGet",
  "classPrivateMethodSet",
  "wrapRegExp"
];

export async function createBenchmarkFixture(rootDir, shape) {
  const context = join(rootDir, shape.name);
  await rm(context, { recursive: true, force: true });
  await mkdir(context, { recursive: true });

  if (shape.kind === "loader") {
    await writeLoaderFixture(context, shape);
  } else {
    await writeWebpackAllFixture(context, shape);
  }

  return {
    name: shape.name,
    context,
    entry: "./src/index.js",
    expectedChecksum: shape.expectedChecksum,
    requiresWebpackLoaders: shape.kind === "loader",
    warmBuildMutationApplied: false
  };
}

export async function applyWarmBuildMutation(fixture) {
  if (fixture.warmBuildMutationApplied) {
    return fixture;
  }

  fixture.expectedChecksum += WARM_BUILD_CHECKSUM_DELTA;
  if (fixture.requiresWebpackLoaders) {
    await writeSource(
      join(fixture.context, "src/loader-data/item0.benchdata"),
      `${1 + WARM_BUILD_CHECKSUM_DELTA}\n`
    );
  } else {
    await writeSource(
      join(fixture.context, "src/__benchmark_checksum.js"),
      checksumSource(fixture.expectedChecksum)
    );
  }
  fixture.warmBuildMutationApplied = true;
  return fixture;
}

async function writeWebpackAllFixture(context, shape) {
  await writeJson(join(context, "package.json"), {
    private: true,
    benchmarkCase: {
      source: "webpack/benchmark/cases/all",
      commit: WEBPACK_ALL_COMMIT,
      loaderOverlay: shape.kind === "loader"
    },
    dependencies: WEBPACK_ALL_DEPENDENCIES
  });
  await writeSource(join(context, "tsconfig.json"), tsconfigSource());
  await writeSource(join(context, "webpack.config.js"), webpackConfigSource());
  await writeSource(join(context, "src/.gitignore"), "copy*\nrome\n");
  await writeSource(join(context, "src/index.js"), webpackAllEntrySource(shape));
  await writeSource(join(context, "src/babel-runtime.js"), babelRuntimeSource());
  await writeSource(join(context, "src/rome.ts"), romeEntrySource());
  await writeSource(
    join(context, "src/__benchmark_checksum.js"),
    checksumSource(LARGE_BASE_CHECKSUM)
  );

  await writeWebpackAllPackages(context);
  await writeThreeCopies(context);
  await writeRomeTree(context);
}

async function writeLoaderFixture(context, shape) {
  await writeWebpackAllFixture(context, shape);
  await writeSource(
    join(context, "loaders/benchmark-loader.cjs"),
    benchmarkLoaderSource()
  );

  for (let index = 0; index < shape.moduleCount; index += 1) {
    await writeSource(
      join(context, `src/loader-data/item${index}.benchdata`),
      `${index + 1}\n`
    );
  }
}

async function writeWebpackAllPackages(context) {
  for (const packageName of WEBPACK_ALL_PACKAGES) {
    await writePackageStub(context, packageName);
  }

  await writeSource(
    join(context, "node_modules/date-fns/esm.js"),
    packageModuleSource("date-fns/esm")
  );
  await writeSource(
    join(context, "node_modules/date-fns/esm/index.js"),
    packageModuleSource("date-fns/esm")
  );
  await writeSource(
    join(context, "node_modules/underscore/modules/index-all.js"),
    packageModuleSource("underscore/modules/index-all")
  );
  await writeBabelRuntimePackage(context);
}

async function writePackageStub(context, packageName) {
  const root = join(context, "node_modules", ...packageName.split("/"));
  await writeJson(join(root, "package.json"), {
    name: packageName,
    version: WEBPACK_ALL_DEPENDENCIES[packageName]?.replace(/^[^\d]*/, "") ?? "1.0.0",
    main: "./index.js",
    module: "./index.js",
    sideEffects: true
  });
  await writeSource(join(root, "index.js"), packageModuleSource(packageName));
}

async function writeBabelRuntimePackage(context) {
  const root = join(context, "node_modules/@babel/runtime");
  await writeJson(join(root, "package.json"), {
    name: "@babel/runtime",
    version: WEBPACK_ALL_DEPENDENCIES["@babel/runtime"].replace(/^[^\d]*/, ""),
    main: "./index.js",
    module: "./index.js",
    sideEffects: true
  });
  await writeSource(join(root, "index.js"), packageModuleSource("@babel/runtime"));
  await writeSource(
    join(root, "package-info.js"),
    "export default { name: '@babel/runtime', version: '7.12.13' };\n"
  );
  await writeSource(join(root, "regenerator.js"), helperModuleSource("regenerator"));

  for (const helper of BABEL_RUNTIME_HELPERS) {
    await writeSource(
      join(root, `helpers/${helper}.js`),
      helperModuleSource(helper)
    );
    await writeSource(
      join(root, `helpers/esm/${helper}.js`),
      helperModuleSource(`esm/${helper}`)
    );
  }
}

async function writeThreeCopies(context) {
  for (let copy = 1; copy <= THREE_COPY_COUNT; copy += 1) {
    const source = [];
    for (let part = 0; part < THREE_PARTS_PER_COPY; part += 1) {
      source.push(`export * from "./parts/part${part}.js";`);
    }
    source.push(`export const threeCopyId = ${copy};`);
    source.push(
      `export const threeChecksum = ${copy} + ${Array.from(
        { length: THREE_PARTS_PER_COPY },
        (_unused, part) => `part${part}Value`
      ).join(" + ")};`
    );
    source.splice(
      0,
      0,
      ...Array.from(
        { length: THREE_PARTS_PER_COPY },
        (_unused, part) =>
          `import { part${part}Value } from "./parts/part${part}.js";`
      )
    );
    await writeSource(join(context, `src/copy${copy}/Three.js`), `${source.join("\n")}\n`);

    for (let part = 0; part < THREE_PARTS_PER_COPY; part += 1) {
      await writeSource(
        join(context, `src/copy${copy}/parts/part${part}.js`),
        [
          `export const part${part}Value = ${copy * 1000 + part};`,
          `export function part${part}Vector(input = 0) {`,
          `  return [input, ${copy}, ${part}, part${part}Value];`,
          "}"
        ].join("\n") + "\n"
      );
    }
  }
}

async function writeRomeTree(context) {
  const imports = [];
  const values = [];
  for (let index = 0; index < ROME_MODULE_COUNT; index += 1) {
    imports.push(`import { romeValue${index} } from "../generated/module${index}.js";`);
    values.push(`romeValue${index}`);
    await writeSource(
      join(context, `src/rome/internal/generated/module${index}.js`),
      [
        `export const romeValue${index} = ${index + 1};`,
        `export function romeTask${index}(input = 0) {`,
        `  return input + romeValue${index};`,
        "}"
      ].join("\n") + "\n"
    );
  }

  await writeSource(
    join(context, "src/rome/internal/cli/cli.js"),
    [
      ...imports,
      "",
      `export const romeChecksum = ${values.join(" + ")};`,
      "export default romeChecksum;"
    ].join("\n") + "\n"
  );
}

function webpackAllEntrySource(shape) {
  const copyImports = [];
  for (let copy = 1; copy <= THREE_COPY_COUNT; copy += 1) {
    copyImports.push(`import * as copy${copy} from "./copy${copy}/Three.js";`);
    copyImports.push(`export { copy${copy} };`);
  }

  return `// Based on webpack/benchmark cases/all/src/index.js at ${WEBPACK_ALL_COMMIT}.
// The benchmark fixture vendors deterministic local package/setup stubs so it can
// run without network access during benchmark execution.

// common-libs
import "./babel-runtime";
import * as core from "@material-ui/core";
import * as lab from "@material-ui/lab";
import * as icons from "@material-ui/icons";
console.log(core, lab, icons);
import "acorn";
import "classnames";
import * as dateFn from "date-fns";
import * as dateFnEsm from "date-fns/esm";
console.log(dateFn, dateFnEsm);
import "jquery";
import "lodash";
import * as lodashEs from "lodash-es";
console.log(lodashEs);
import "moment";
import "react";
import "react-dom";
import "redux";
import * as rxjs from "rxjs";
console.log(rxjs);
import * as underscore from "underscore";
import * as underscoreModules from "underscore/modules/index-all";
console.log(underscore, underscoreModules);
import { NIL, parse, stringify, v1, v3, v4, v5, validate, version } from "uuid";
console.log(NIL, parse, stringify, v1, v3, v4, v5, validate, version);
import "vue";
import "zone.js";

// common-libs-chunks
import("./babel-runtime");
import("@material-ui/core");
import("@material-ui/lab");
import("@material-ui/icons");
import("acorn");
import("classnames");
import("date-fns");
import("date-fns/esm");
import("jquery");
import("lodash");
import("lodash-es");
import("moment");
import("react");
import("react-dom");
import("redux");
import("rxjs");
import("underscore");
import("underscore/modules/index-all");
import("uuid");
import("vue");
import("zone.js");

// atlaskit-editor
// benchmark from parcel-benchmark
import React from "react";
import ReactDOM from "react-dom";
import { Editor } from "@atlaskit/editor-core";

ReactDOM.render(
  React.createElement(Editor, {
    placeholder: "editor",
    appearance: "comment",
    test: "Hello World"
  }),
  document.getElementById("react-root")
);

// esbuild-three
${copyImports.join("\n")}

// rome
import "./rome.ts";

import { benchmarkChecksum } from "./__benchmark_checksum.js";
${loaderEntryImports(shape)}

${loaderChecksumSource(shape)}
export const checksum = benchmarkChecksum + loaderChecksum;
export default { checksum };

console.log("Hello World", checksum);
`;
}

function babelRuntimeSource() {
  const imports = [];
  const values = [];
  for (const [index, helper] of BABEL_RUNTIME_HELPERS.entries()) {
    imports.push(`import helper${index} from "@babel/runtime/helpers/${helper}";`);
    imports.push(`import esmHelper${index} from "@babel/runtime/helpers/esm/${helper}";`);
    values.push(`helper${index}`, `esmHelper${index}`);
  }
  imports.push('import runtimePackage from "@babel/runtime/package-info";');
  imports.push('import regenerator from "@babel/runtime/regenerator";');
  values.push("runtimePackage", "regenerator");

  return `// Based on webpack/benchmark cases/all/src/babel-runtime.js at ${WEBPACK_ALL_COMMIT}.
${imports.join("\n")}

console.log(
  ${values.join(",\n  ")}
);
`;
}

function romeEntrySource() {
  return `import "./rome/internal/cli/cli";

console.log("Hello World");
`;
}

function loaderEntryImports(shape) {
  if (shape.kind !== "loader") {
    return "";
  }

  const imports = [];
  for (let index = 0; index < shape.moduleCount; index += 1) {
    imports.push(`import value${index} from "./loader-data/item${index}.benchdata";`);
  }

  return `
// webpack-compatible loader overlay
${imports.join("\n")}
`;
}

function loaderChecksumSource(shape) {
  if (shape.kind !== "loader") {
    return "const loaderChecksum = 0;";
  }

  const values = [];
  for (let index = 0; index < shape.moduleCount; index += 1) {
    values.push(`value${index}`);
  }

  return `const loaderValues = [
  ${values.join(",\n  ")}
];

const loaderChecksum = loaderValues.reduce((total, value) => total + value, 0);`;
}

function benchmarkLoaderSource() {
  return `module.exports = function benchmarkLoader(source) {
  const value = Number.parseInt(String(source).trim(), 10);
  if (!Number.isFinite(value)) {
    throw new Error("benchmark loader expected a numeric payload");
  }
  return [
    \`const value = \${value};\`,
    "export default value;",
    "export { value as loadedValue };"
  ].join("\\n");
};
`;
}

function packageModuleSource(packageName) {
  return `const packageName = ${JSON.stringify(packageName)};

export function createElement(type, props, ...children) {
  return { type, props, children };
}

export function render() {
  return null;
}

export function Editor(props = {}) {
  return { type: "Editor", props };
}

export const NIL = "00000000-0000-0000-0000-000000000000";
export const checksum = packageName.length;

export function parse(value = "") {
  return String(value).split("");
}

export function stringify(value = "") {
  return String(value);
}

export function v1() {
  return \`\${packageName}:v1\`;
}

export function v3() {
  return \`\${packageName}:v3\`;
}

export function v4() {
  return \`\${packageName}:v4\`;
}

export function v5() {
  return \`\${packageName}:v5\`;
}

export function validate(value) {
  return typeof value === "string";
}

export function version() {
  return packageName;
}

export default {
  packageName,
  checksum,
  createElement,
  render,
  Editor,
  NIL,
  parse,
  stringify,
  v1,
  v3,
  v4,
  v5,
  validate,
  version
};
`;
}

function helperModuleSource(helper) {
  return `export default function helper(value) {
  return value ?? ${JSON.stringify(helper)};
}

export const helperName = ${JSON.stringify(helper)};
`;
}

function checksumSource(checksum) {
  return `export const benchmarkChecksum = ${checksum};\n`;
}

function loaderExpectedChecksum(moduleCount) {
  return (moduleCount * (moduleCount + 1)) / 2;
}

function tsconfigSource() {
  return `${JSON.stringify(
    {
      compilerOptions: {
        noEmit: true,
        esModuleInterop: true,
        resolveJsonModule: true,
        moduleResolution: "node",
        target: "es2019",
        module: "esnext",
        baseUrl: "."
      },
      include: ["./src/rome.ts"],
      paths: {
        "@internal/*": ["src/rome/internal/*"],
        "@internal/virtual-*": ["src/rome/internal/virtual-packages/*"],
        rome: ["src/rome/internal/virtual-packages/rome"]
      }
    },
    null,
    2
  )}\n`;
}

function webpackConfigSource() {
  return `const { resolve } = require("path");

module.exports = {
  resolve: {
    extensions: [".ts", ".tsx", ".js"],
    alias: {
      "@internal": resolve(__dirname, "src/rome/internal")
    }
  },
  output: {
    hashFunction: "xxhash64"
  },
  optimization: {
    sideEffects: false
  },
  experiments: {
    cacheUnaffected: true
  },
  module: {
    unsafeCache: true
  }
};
`;
}

async function writeSource(path, source) {
  await mkdir(dirname(path), { recursive: true });
  await writeFile(path, source, "utf8");
}

async function writeJson(path, value) {
  await mkdir(dirname(path), { recursive: true });
  await writeFile(path, `${JSON.stringify(value, null, 2)}\n`, "utf8");
}
