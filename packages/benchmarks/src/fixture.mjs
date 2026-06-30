import { mkdir, rm, utimes, writeFile } from "node:fs/promises";
import { dirname, join } from "node:path";

const WARM_BUILD_MUTATION_FEATURE = 0;
const WARM_BUILD_MUTATION_DELTA = 1000;

export const FIXTURE_SHAPES = {
  small: {
    name: "small",
    featureModules: 10,
    sharedModules: 5,
    importsPerFeature: 3
  },
  medium: {
    name: "medium",
    featureModules: 100,
    sharedModules: 20,
    importsPerFeature: 4
  },
  large: {
    name: "large",
    featureModules: 500,
    sharedModules: 50,
    importsPerFeature: 5
  }
};

export async function createBenchmarkFixture(rootDir, shape) {
  const context = join(rootDir, shape.name);
  await rm(context, { recursive: true, force: true });
  await mkdir(context, { recursive: true });
  await writeJson(join(context, "package.json"), {
    private: true,
    type: "module"
  });

  await writeSource(join(context, "src/index.js"), entrySource(shape));

  for (let shared = 0; shared < shape.sharedModules; shared += 1) {
    await writeSource(
      join(context, `src/shared/shared${shared}.js`),
      sharedSource(shared)
    );
  }

  for (let feature = 0; feature < shape.featureModules; feature += 1) {
    await writeSource(
      join(context, `src/features/feature${feature}.js`),
      featureSource(shape, feature)
    );
    await writeSource(
      join(context, `src/features/leaf${feature}.js`),
      leafSource(feature)
    );
    await writeSource(
      join(context, `src/features/reexport${feature}.js`),
      reexportSource(feature)
    );
  }

  return {
    name: shape.name,
    context,
    entry: "./src/index.js",
    expectedChecksum: expectedChecksum(shape)
  };
}

export async function applyWarmBuildMutation(fixture) {
  if (fixture.warmBuildMutation) {
    return fixture;
  }

  const feature = WARM_BUILD_MUTATION_FEATURE;
  const path = join(fixture.context, `src/features/leaf${feature}.js`);
  await writeSource(
    path,
    leafSource(feature, feature + 1 + WARM_BUILD_MUTATION_DELTA)
  );

  const modifiedAt = new Date(Date.now() + 1000);
  await utimes(path, modifiedAt, modifiedAt);

  return {
    ...fixture,
    expectedChecksum: fixture.expectedChecksum + WARM_BUILD_MUTATION_DELTA * 2,
    warmBuildMutation: {
      path
    }
  };
}

function entrySource(shape) {
  const source = [];
  for (let feature = 0; feature < shape.featureModules; feature += 1) {
    source.push(
      `import { feature${feature}, reexport${feature} } from "./features/feature${feature}.js";`
    );
  }

  source.push("const values = [");
  for (let feature = 0; feature < shape.featureModules; feature += 1) {
    source.push(`  feature${feature}, reexport${feature},`);
  }
  source.push("];");
  source.push(
    "export const checksum = values.reduce((total, value) => total + value, 0);"
  );
  source.push("export default checksum;");
  return `${source.join("\n")}\n`;
}

function featureSource(shape, feature) {
  const sharedImports = sharedImportsFor(shape, feature);
  const source = [];
  for (const shared of sharedImports) {
    source.push(
      `import { shared${shared} } from "../shared/shared${shared}.js";`
    );
  }
  source.push(`import { leaf${feature} } from "./leaf${feature}.js";`);
  source.push(
    `export { leaf${feature} as reexport${feature} } from "./reexport${feature}.js";`
  );
  source.push(`export * from "../shared/shared${feature % shape.sharedModules}.js";`);
  source.push(
    `export const feature${feature} = leaf${feature}${sharedImports
      .map((shared) => ` + shared${shared}`)
      .join("")};`
  );
  return `${source.join("\n")}\n`;
}

function sharedImportsFor(shape, feature) {
  return Array.from(
    { length: shape.importsPerFeature },
    (_unused, offset) => (feature + offset) % shape.sharedModules
  );
}

function sharedSource(shared) {
  return `export const shared${shared} = ${shared + 1};\nexport const sharedValue${shared} = shared${shared} * 2;\n`;
}

function leafSource(feature, value = feature + 1) {
  return `export const leaf${feature} = ${value};\n`;
}

function reexportSource(feature) {
  return `export { leaf${feature} } from "./leaf${feature}.js";\n`;
}

function expectedChecksum(shape) {
  let checksum = 0;
  for (let feature = 0; feature < shape.featureModules; feature += 1) {
    const leaf = feature + 1;
    const sharedTotal = sharedImportsFor(shape, feature).reduce(
      (total, shared) => total + shared + 1,
      0
    );
    checksum += leaf + sharedTotal + leaf;
  }
  return checksum;
}

async function writeSource(path, source) {
  await mkdir(dirname(path), { recursive: true });
  await writeFile(path, source, "utf8");
}

async function writeJson(path, value) {
  await mkdir(dirname(path), { recursive: true });
  await writeFile(path, `${JSON.stringify(value, null, 2)}\n`, "utf8");
}
