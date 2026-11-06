const REACT_ELEMENT_TYPE = Symbol.for("react.element");

function isSpecial(value) {
  const tag = Object.prototype.toString.call(value);
  return tag === "[object RegExp]" || tag === "[object Date]" || value.$$typeof === REACT_ELEMENT_TYPE;
}

function defaultIsMergeableObject(value) {
  return !!value && typeof value === "object" && !isSpecial(value);
}

function emptyTarget(value) {
  return Array.isArray(value) ? [] : {};
}

function cloneUnlessOtherwiseSpecified(value, options) {
  return options.clone !== false && options.isMergeableObject(value) ? deepmerge(emptyTarget(value), value, options) : value;
}

function defaultArrayMerge(target, source, options) {
  return target.concat(source).map((element) => cloneUnlessOtherwiseSpecified(element, options));
}

function getMergeFunction(key, options) {
  if (!options.customMerge) return deepmerge;
  const customMerge = options.customMerge(key);
  return typeof customMerge === "function" ? customMerge : deepmerge;
}

function getKeys(target) {
  const symbols = Object.getOwnPropertySymbols(target).filter((symbol) => Object.prototype.propertyIsEnumerable.call(target, symbol));
  return Object.keys(target).concat(symbols);
}

function propertyIsOnObject(object, property) {
  try {
    return property in object;
  } catch {
    return false;
  }
}

function propertyIsUnsafe(target, key) {
  return (
    key === "__proto__" ||
    (propertyIsOnObject(target, key) &&
      !(Object.hasOwn(target, key) && Object.prototype.propertyIsEnumerable.call(target, key)))
  );
}

function mergeObject(target, source, options) {
  const destination = {};
  if (options.isMergeableObject(target)) {
    for (const key of getKeys(target)) {
      if (key === "__proto__") continue;
      destination[key] = cloneUnlessOtherwiseSpecified(target[key], options);
    }
  }
  for (const key of getKeys(source)) {
    if (propertyIsUnsafe(target, key)) continue;
    if (propertyIsOnObject(target, key) && options.isMergeableObject(source[key])) {
      destination[key] = getMergeFunction(key, options)(target[key], source[key], options);
    } else {
      destination[key] = cloneUnlessOtherwiseSpecified(source[key], options);
    }
  }
  return destination;
}

function deepmerge(target, source, options) {
  const opts = options || {};
  opts.arrayMerge = opts.arrayMerge || defaultArrayMerge;
  opts.isMergeableObject = opts.isMergeableObject || defaultIsMergeableObject;
  opts.cloneUnlessOtherwiseSpecified = cloneUnlessOtherwiseSpecified;

  const sourceIsArray = Array.isArray(source);
  const targetIsArray = Array.isArray(target);

  if (sourceIsArray !== targetIsArray) return cloneUnlessOtherwiseSpecified(source, opts);
  if (sourceIsArray) return opts.arrayMerge(target, source, opts);
  return mergeObject(target, source, opts);
}

deepmerge.all = function deepmergeAll(array, options) {
  if (!Array.isArray(array)) throw new Error("first argument should be an array");
  return array.reduce((prev, next) => deepmerge(prev, next, options), {});
};

module.exports = deepmerge;
