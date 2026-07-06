"use strict";

const { createRequire } = require("node:module");
const { dirname } = require("node:path");

const metroRoot = dirname(require.resolve("metro/package.json"));
const metroRequire = createRequire(`${metroRoot}/package.json`);
const metroBabelTransformer = metroRequire("metro-babel-transformer");
const transformModulesCommonJs = require("@babel/plugin-transform-modules-commonjs");

function transform(args) {
  return metroBabelTransformer.transform({
    ...args,
    plugins: [...(args.plugins ?? []), transformModulesCommonJs]
  });
}

function getCacheKey(options) {
  return `${metroBabelTransformer.getCacheKey(options)}:unpack-benchmark-commonjs-v1`;
}

module.exports = {
  ...metroBabelTransformer,
  transform,
  getCacheKey
};
