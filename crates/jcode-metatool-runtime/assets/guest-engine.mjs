var __require = /* @__PURE__ */ ((x) => typeof require !== "undefined" ? require : typeof Proxy !== "undefined" ? new Proxy(x, {
  get: (a, b) => (typeof require !== "undefined" ? require : a)[b]
}) : x)(function(x) {
  if (typeof require !== "undefined") return require.apply(this, arguments);
  throw Error('Dynamic require of "' + x + '" is not supported');
});

// node_modules/effect/dist/Pipeable.js
var pipeArguments = (self2, args2) => {
  switch (args2.length) {
    case 0:
      return self2;
    case 1:
      return args2[0](self2);
    case 2:
      return args2[1](args2[0](self2));
    case 3:
      return args2[2](args2[1](args2[0](self2)));
    case 4:
      return args2[3](args2[2](args2[1](args2[0](self2))));
    case 5:
      return args2[4](args2[3](args2[2](args2[1](args2[0](self2)))));
    case 6:
      return args2[5](args2[4](args2[3](args2[2](args2[1](args2[0](self2))))));
    case 7:
      return args2[6](args2[5](args2[4](args2[3](args2[2](args2[1](args2[0](self2)))))));
    case 8:
      return args2[7](args2[6](args2[5](args2[4](args2[3](args2[2](args2[1](args2[0](self2))))))));
    case 9:
      return args2[8](args2[7](args2[6](args2[5](args2[4](args2[3](args2[2](args2[1](args2[0](self2)))))))));
    default: {
      let ret = self2;
      for (let i = 0, len = args2.length; i < len; i++) {
        ret = args2[i](ret);
      }
      return ret;
    }
  }
};
var Prototype = {
  pipe() {
    return pipeArguments(this, arguments);
  }
};
var Class = /* @__PURE__ */ (function() {
  function PipeableBase() {
  }
  PipeableBase.prototype = Prototype;
  return PipeableBase;
})();

// node_modules/effect/dist/Function.js
var dual = function(arity, body) {
  if (typeof arity === "function") {
    return function() {
      return arity(arguments) ? body.apply(this, arguments) : (self2) => body(self2, ...arguments);
    };
  }
  switch (arity) {
    case 0:
    case 1:
      throw new RangeError(`Invalid arity ${arity}`);
    case 2:
      return function(a, b) {
        if (arguments.length >= 2) {
          return body(a, b);
        }
        return function(self2) {
          return body(self2, a);
        };
      };
    case 3:
      return function(a, b, c) {
        if (arguments.length >= 3) {
          return body(a, b, c);
        }
        return function(self2) {
          return body(self2, a, b);
        };
      };
    default:
      return function() {
        if (arguments.length >= arity) {
          return body.apply(this, arguments);
        }
        const args2 = arguments;
        return function(self2) {
          return body(self2, ...args2);
        };
      };
  }
};
var identity = (a) => a;
var constant = (value) => () => value;
var constTrue = /* @__PURE__ */ constant(true);
var constFalse = /* @__PURE__ */ constant(false);
var constUndefined = /* @__PURE__ */ constant(void 0);
var constVoid = constUndefined;
function pipe(a, ...args2) {
  return pipeArguments(a, args2);
}
function memoize(f) {
  const cache = /* @__PURE__ */ new WeakMap();
  return (a) => {
    if (cache.has(a)) {
      return cache.get(a);
    }
    const result2 = f(a);
    cache.set(a, result2);
    return result2;
  };
}

// node_modules/effect/dist/internal/equal.js
var getAllObjectKeys = (obj) => {
  const keys = new Set(Reflect.ownKeys(obj));
  if (obj.constructor === Object) return keys;
  if (obj instanceof Error) {
    keys.delete("stack");
  }
  const proto = Object.getPrototypeOf(obj);
  let current = proto;
  while (current !== null && current !== Object.prototype) {
    const ownKeys = Reflect.ownKeys(current);
    for (let i = 0; i < ownKeys.length; i++) {
      keys.add(ownKeys[i]);
    }
    current = Object.getPrototypeOf(current);
  }
  if (keys.has("constructor") && typeof obj.constructor === "function" && proto === obj.constructor.prototype) {
    keys.delete("constructor");
  }
  return keys;
};
var byReferenceInstances = /* @__PURE__ */ new WeakSet();

// node_modules/effect/dist/Predicate.js
function isString(input) {
  return typeof input === "string";
}
function isNumber(input) {
  return typeof input === "number";
}
function isBoolean(input) {
  return typeof input === "boolean";
}
function isSymbol(input) {
  return typeof input === "symbol";
}
function isPropertyKey(u) {
  return isString(u) || isNumber(u) || isSymbol(u);
}
function isFunction(input) {
  return typeof input === "function";
}
function isNotUndefined(input) {
  return input !== void 0;
}
function isNotNullish(input) {
  return input != null;
}
function isUnknown(_) {
  return true;
}
function isObject(input) {
  return typeof input === "object" && input !== null && !Array.isArray(input);
}
function isObjectKeyword(input) {
  return typeof input === "object" && input !== null || isFunction(input);
}
var hasProperty = /* @__PURE__ */ dual(2, (self2, property) => isObjectKeyword(self2) && property in self2);
var isTagged = /* @__PURE__ */ dual(2, (self2, tag2) => hasProperty(self2, "_tag") && self2["_tag"] === tag2);
function isError(input) {
  return input instanceof Error;
}
function isIterable(input) {
  return hasProperty(input, Symbol.iterator) || isString(input);
}

// node_modules/effect/dist/Hash.js
var symbol = "~effect/interfaces/Hash";
var hash = (self2) => {
  switch (typeof self2) {
    case "number":
      return number(self2);
    case "bigint":
      return string(self2.toString(10));
    case "boolean":
      return string(String(self2));
    case "symbol":
      return string(String(self2));
    case "string":
      return string(self2);
    case "undefined":
      return string("undefined");
    case "function":
    case "object": {
      if (self2 === null) {
        return string("null");
      } else if (self2 instanceof Date) {
        return string(self2.toISOString());
      } else if (self2 instanceof RegExp) {
        return string(self2.toString());
      } else {
        if (byReferenceInstances.has(self2)) {
          return random(self2);
        }
        if (hashCache.has(self2)) {
          return hashCache.get(self2);
        }
        const h = withVisitedTracking(self2, () => {
          if (isHash(self2)) {
            return self2[symbol]();
          } else if (typeof self2 === "function") {
            return random(self2);
          } else if (Array.isArray(self2) || ArrayBuffer.isView(self2)) {
            return array(self2);
          } else if (self2 instanceof Map) {
            return hashMap(self2);
          } else if (self2 instanceof Set) {
            return hashSet(self2);
          }
          return structure(self2);
        });
        hashCache.set(self2, h);
        return h;
      }
    }
    default:
      throw new Error(`BUG: unhandled typeof ${typeof self2} - please report an issue at https://github.com/Effect-TS/effect/issues`);
  }
};
var random = (self2) => {
  if (!randomHashCache.has(self2)) {
    randomHashCache.set(self2, number(Math.floor(Math.random() * Number.MAX_SAFE_INTEGER)));
  }
  return randomHashCache.get(self2);
};
var combine = /* @__PURE__ */ dual(2, (self2, b) => self2 * 53 ^ b);
var optimize = (n) => n & 3221225471 | n >>> 1 & 1073741824;
var isHash = (u) => hasProperty(u, symbol);
var number = (n) => {
  if (n !== n) {
    return string("NaN");
  }
  if (n === Infinity) {
    return string("Infinity");
  }
  if (n === -Infinity) {
    return string("-Infinity");
  }
  let h = n | 0;
  if (h !== n) {
    h ^= n * 4294967295;
  }
  while (n > 4294967295) {
    h ^= n /= 4294967295;
  }
  return optimize(h);
};
var string = (str) => {
  let h = 5381, i = str.length;
  while (i) {
    h = h * 33 ^ str.charCodeAt(--i);
  }
  return optimize(h);
};
var structureKeys = (o, keys) => {
  let h = 12289;
  for (const key of keys) {
    h ^= combine(hash(key), hash(o[key]));
  }
  return optimize(h);
};
var structure = (o) => structureKeys(o, getAllObjectKeys(o));
var iterableWith = (seed, f) => (iter) => {
  let h = seed;
  for (const element of iter) {
    h ^= f(element);
  }
  return optimize(h);
};
var array = /* @__PURE__ */ iterableWith(6151, hash);
var hashMap = /* @__PURE__ */ iterableWith(/* @__PURE__ */ string("Map"), ([k, v]) => combine(hash(k), hash(v)));
var hashSet = /* @__PURE__ */ iterableWith(/* @__PURE__ */ string("Set"), hash);
var randomHashCache = /* @__PURE__ */ new WeakMap();
var hashCache = /* @__PURE__ */ new WeakMap();
var visitedObjects = /* @__PURE__ */ new WeakSet();
function withVisitedTracking(obj, fn2) {
  if (visitedObjects.has(obj)) {
    return string("[Circular]");
  }
  visitedObjects.add(obj);
  const result2 = fn2();
  visitedObjects.delete(obj);
  return result2;
}

// node_modules/effect/dist/Equal.js
var symbol2 = "~effect/interfaces/Equal";
function equals() {
  if (arguments.length === 1) {
    return (self2) => compareBoth(self2, arguments[0]);
  }
  return compareBoth(arguments[0], arguments[1]);
}
function compareBoth(self2, that) {
  if (self2 === that) return true;
  if (self2 == null || that == null) return false;
  const selfType = typeof self2;
  if (selfType !== typeof that) {
    return false;
  }
  if (selfType === "number" && self2 !== self2 && that !== that) {
    return true;
  }
  if (selfType !== "object" && selfType !== "function") {
    return false;
  }
  if (byReferenceInstances.has(self2) || byReferenceInstances.has(that)) {
    return false;
  }
  return withCache(self2, that, compareObjects);
}
function withVisitedTracking2(self2, that, fn2) {
  const hasLeft = visitedLeft.has(self2);
  const hasRight = visitedRight.has(that);
  if (hasLeft && hasRight) {
    return true;
  }
  if (hasLeft || hasRight) {
    return false;
  }
  visitedLeft.add(self2);
  visitedRight.add(that);
  const result2 = fn2();
  visitedLeft.delete(self2);
  visitedRight.delete(that);
  return result2;
}
var visitedLeft = /* @__PURE__ */ new WeakSet();
var visitedRight = /* @__PURE__ */ new WeakSet();
function compareObjects(self2, that) {
  if (hash(self2) !== hash(that)) {
    return false;
  } else if (self2 instanceof Date) {
    if (!(that instanceof Date)) return false;
    return self2.toISOString() === that.toISOString();
  } else if (self2 instanceof RegExp) {
    if (!(that instanceof RegExp)) return false;
    return self2.toString() === that.toString();
  }
  const selfIsEqual = isEqual(self2);
  const thatIsEqual = isEqual(that);
  if (selfIsEqual !== thatIsEqual) return false;
  const bothEquals = selfIsEqual && thatIsEqual;
  if (typeof self2 === "function" && !bothEquals) {
    return false;
  }
  return withVisitedTracking2(self2, that, () => {
    if (bothEquals) {
      return self2[symbol2](that);
    } else if (Array.isArray(self2)) {
      if (!Array.isArray(that) || self2.length !== that.length) {
        return false;
      }
      return compareArrays(self2, that);
    } else if (ArrayBuffer.isView(self2)) {
      if (!ArrayBuffer.isView(that) || self2.byteLength !== that.byteLength) {
        return false;
      }
      return compareTypedArrays(self2, that);
    } else if (self2 instanceof Map) {
      if (!(that instanceof Map) || self2.size !== that.size) {
        return false;
      }
      return compareMaps(self2, that);
    } else if (self2 instanceof Set) {
      if (!(that instanceof Set) || self2.size !== that.size) {
        return false;
      }
      return compareSets(self2, that);
    }
    return compareRecords(self2, that);
  });
}
function withCache(self2, that, f) {
  let selfMap = equalityCache.get(self2);
  if (!selfMap) {
    selfMap = /* @__PURE__ */ new WeakMap();
    equalityCache.set(self2, selfMap);
  } else if (selfMap.has(that)) {
    return selfMap.get(that);
  }
  const result2 = f(self2, that);
  selfMap.set(that, result2);
  let thatMap = equalityCache.get(that);
  if (!thatMap) {
    thatMap = /* @__PURE__ */ new WeakMap();
    equalityCache.set(that, thatMap);
  }
  thatMap.set(self2, result2);
  return result2;
}
var equalityCache = /* @__PURE__ */ new WeakMap();
function compareArrays(self2, that) {
  for (let i = 0; i < self2.length; i++) {
    if (!compareBoth(self2[i], that[i])) {
      return false;
    }
  }
  return true;
}
function compareTypedArrays(self2, that) {
  if (self2.length !== that.length) {
    return false;
  }
  for (let i = 0; i < self2.length; i++) {
    if (self2[i] !== that[i]) {
      return false;
    }
  }
  return true;
}
function compareRecords(self2, that) {
  const selfKeys = getAllObjectKeys(self2);
  const thatKeys = getAllObjectKeys(that);
  if (selfKeys.size !== thatKeys.size) {
    return false;
  }
  for (const key of selfKeys) {
    if (!thatKeys.has(key) || !compareBoth(self2[key], that[key])) {
      return false;
    }
  }
  return true;
}
function makeCompareMap(keyEquivalence, valueEquivalence) {
  return function compareMaps2(self2, that) {
    for (const [selfKey, selfValue] of self2) {
      let found = false;
      for (const [thatKey, thatValue] of that) {
        if (keyEquivalence(selfKey, thatKey) && valueEquivalence(selfValue, thatValue)) {
          found = true;
          break;
        }
      }
      if (!found) {
        return false;
      }
    }
    return true;
  };
}
var compareMaps = /* @__PURE__ */ makeCompareMap(compareBoth, compareBoth);
function makeCompareSet(equivalence) {
  return function compareSets2(self2, that) {
    for (const selfValue of self2) {
      let found = false;
      for (const thatValue of that) {
        if (equivalence(selfValue, thatValue)) {
          found = true;
          break;
        }
      }
      if (!found) {
        return false;
      }
    }
    return true;
  };
}
var compareSets = /* @__PURE__ */ makeCompareSet(compareBoth);
var isEqual = (u) => hasProperty(u, symbol2);
var asEquivalence = () => equals;

// node_modules/effect/dist/Redactable.js
var symbolRedactable = /* @__PURE__ */ Symbol.for("~effect/Redactable");
var isRedactable = (u) => hasProperty(u, symbolRedactable);
function redact(u) {
  if (isRedactable(u)) return getRedacted(u);
  return u;
}
function getRedacted(redactable) {
  return redactable[symbolRedactable](globalThis[currentFiberTypeId]?.context ?? emptyContext);
}
var currentFiberTypeId = "~effect/Fiber/currentFiber";
var emptyContext = {
  "~effect/Context": {},
  mapUnsafe: /* @__PURE__ */ new Map(),
  pipe() {
    return pipeArguments(this, arguments);
  }
};

// node_modules/effect/dist/Formatter.js
function format(input, options) {
  const space = options?.space ?? 0;
  const seen = /* @__PURE__ */ new WeakSet();
  const gap = !space ? "" : typeof space === "number" ? " ".repeat(space) : space;
  const ind = (d) => gap.repeat(d);
  const wrap = (v, body) => {
    const ctor = v?.constructor;
    return ctor && ctor !== Object.prototype.constructor && ctor.name ? `${ctor.name}(${body})` : body;
  };
  const ownKeys = (o) => {
    try {
      return Reflect.ownKeys(o);
    } catch {
      return ["[ownKeys threw]"];
    }
  };
  function recur2(v, d = 0) {
    if (Array.isArray(v)) {
      if (seen.has(v)) return CIRCULAR;
      seen.add(v);
      if (!gap || v.length <= 1) return `[${v.map((x) => recur2(x, d)).join(",")}]`;
      const inner = v.map((x) => recur2(x, d + 1)).join(",\n" + ind(d + 1));
      return `[
${ind(d + 1)}${inner}
${ind(d)}]`;
    }
    if (v instanceof Date) return formatDate(v);
    if (!options?.ignoreToString && hasProperty(v, "toString") && typeof v["toString"] === "function" && v["toString"] !== Object.prototype.toString && v["toString"] !== Array.prototype.toString) {
      const s = safeToString(v);
      if (v instanceof Error && v.cause) {
        return `${s} (cause: ${recur2(v.cause, d)})`;
      }
      return s;
    }
    if (typeof v === "string") return JSON.stringify(v);
    if (typeof v === "number" || v == null || typeof v === "boolean" || typeof v === "symbol") return String(v);
    if (typeof v === "bigint") return String(v) + "n";
    if (typeof v === "object" || typeof v === "function") {
      if (seen.has(v)) return CIRCULAR;
      seen.add(v);
      if (symbolRedactable in v) return format(getRedacted(v));
      if (Symbol.iterator in v) {
        return `${v.constructor.name}(${recur2(Array.from(v), d)})`;
      }
      const keys = ownKeys(v);
      if (!gap || keys.length <= 1) {
        const body2 = `{${keys.map((k) => `${formatPropertyKey(k)}:${recur2(v[k], d)}`).join(",")}}`;
        return wrap(v, body2);
      }
      const body = `{
${keys.map((k) => `${ind(d + 1)}${formatPropertyKey(k)}: ${recur2(v[k], d + 1)}`).join(",\n")}
${ind(d)}}`;
      return wrap(v, body);
    }
    return String(v);
  }
  return recur2(input, 0);
}
var CIRCULAR = "[Circular]";
function formatPropertyKey(name) {
  return typeof name === "string" ? JSON.stringify(name) : String(name);
}
function formatPath(path) {
  return path.map((key) => `[${formatPropertyKey(key)}]`).join("");
}
function formatDate(date) {
  try {
    return date.toISOString();
  } catch {
    return "Invalid Date";
  }
}
function safeToString(input) {
  try {
    const s = input.toString();
    return typeof s === "string" ? s : String(s);
  } catch {
    return "[toString threw]";
  }
}
function formatJson(input, options) {
  const ancestors = [];
  return JSON.stringify(input, function(_key, value) {
    const redacted = redact(value);
    if (typeof redacted !== "object" || redacted === null) {
      return redacted;
    }
    while (ancestors.length > 0 && ancestors[ancestors.length - 1] !== this) {
      ancestors.pop();
    }
    if (ancestors.includes(redacted)) {
      return void 0;
    }
    ancestors.push(redacted);
    return redacted;
  }, options?.space);
}

// node_modules/effect/dist/Inspectable.js
var NodeInspectSymbol = /* @__PURE__ */ Symbol.for("nodejs.util.inspect.custom");
var toJson = (input) => {
  try {
    if (hasProperty(input, "toJSON") && isFunction(input["toJSON"]) && input["toJSON"].length === 0) {
      return input.toJSON();
    } else if (Array.isArray(input)) {
      return input.map(toJson);
    }
  } catch {
    return "[toJSON threw]";
  }
  return redact(input);
};
var toStringUnknown = (u, whitespace = 2) => {
  if (typeof u === "string") {
    return u;
  }
  try {
    return typeof u === "object" ? formatJson(u, {
      space: whitespace
    }) : String(u);
  } catch {
    return String(u);
  }
};
var BaseProto = {
  toJSON() {
    return toJson(this);
  },
  [NodeInspectSymbol]() {
    return this.toJSON();
  },
  toString() {
    return format(this.toJSON());
  }
};
var Class2 = class {
  /**
   * Node.js custom inspection method.
   *
   * **When to use**
   *
   * Use to expose the class JSON representation to Node.js inspection.
   *
   * @since 2.0.0
   */
  [NodeInspectSymbol]() {
    return this.toJSON();
  }
  /**
   * Returns a formatted string representation of this object.
   *
   * **When to use**
   *
   * Use to format the class JSON representation as a string.
   *
   * @since 2.0.0
   */
  toString() {
    return format(this.toJSON());
  }
};

// node_modules/effect/dist/Utils.js
var SingleShotGen = class _SingleShotGen {
  called = false;
  self;
  constructor(self2) {
    this.self = self2;
  }
  /**
   * Yields the stored value once, then completes with the value sent back in.
   *
   * **When to use**
   *
   * Use to advance a `SingleShotGen` through its single yield and completion
   * step.
   *
   * @since 2.0.0
   */
  next(a) {
    return this.called ? {
      value: a,
      done: true
    } : (this.called = true, {
      value: this.self,
      done: false
    });
  }
  /**
   * Creates a fresh single-shot iterator over the stored value.
   *
   * **When to use**
   *
   * Use to iterate the wrapped value again without reusing the consumed
   * iterator state.
   *
   * @since 2.0.0
   */
  [Symbol.iterator]() {
    return new _SingleShotGen(this.self);
  }
};
var pickInternalCall = () => {
  const InternalTypeId = "~effect/Utils/internal";
  const standard = {
    [InternalTypeId]: (body) => {
      return body();
    }
  };
  const forced = {
    [InternalTypeId]: (body) => {
      try {
        return body();
      } finally {
      }
    }
  };
  const isNotOptimizedAway = standard[InternalTypeId](() => new Error().stack)?.includes(InternalTypeId) === true;
  return isNotOptimizedAway ? standard[InternalTypeId] : forced[InternalTypeId];
};
var internalCall = /* @__PURE__ */ pickInternalCall();

// node_modules/effect/dist/internal/core.js
var EffectTypeId = `~effect/Effect`;
var ExitTypeId = `~effect/Exit`;
var effectVariance = {
  _A: identity,
  _E: identity,
  _R: identity
};
var identifier = `${EffectTypeId}/identifier`;
var args = `${EffectTypeId}/args`;
var evaluate = `${EffectTypeId}/evaluate`;
var contA = `${EffectTypeId}/successCont`;
var contE = `${EffectTypeId}/failureCont`;
var contAll = `${EffectTypeId}/ensureCont`;
var Yield = /* @__PURE__ */ Symbol.for("effect/Effect/Yield");
var PipeInspectableProto = {
  pipe() {
    return pipeArguments(this, arguments);
  },
  toJSON() {
    return {
      ...this
    };
  },
  toString() {
    return format(this.toJSON(), {
      ignoreToString: true,
      space: 2
    });
  },
  [NodeInspectSymbol]() {
    return this.toJSON();
  }
};
var StructuralProto = {
  [symbol]() {
    return structureKeys(this, Object.keys(this));
  },
  [symbol2](that) {
    const selfKeys = Object.keys(this);
    const thatKeys = Object.keys(that);
    if (selfKeys.length !== thatKeys.length) return false;
    for (let i = 0; i < selfKeys.length; i++) {
      if (selfKeys[i] !== thatKeys[i] || !equals(this[selfKeys[i]], that[selfKeys[i]])) {
        return false;
      }
    }
    return true;
  }
};
var EffectProto = {
  [EffectTypeId]: effectVariance,
  ...PipeInspectableProto,
  [Symbol.iterator]() {
    return new SingleShotGen(this);
  },
  toJSON() {
    return {
      _id: "Effect",
      op: this[identifier],
      ...args in this ? {
        args: this[args]
      } : void 0
    };
  }
};
var isEffect = (u) => hasProperty(u, EffectTypeId);
var isExit = (u) => hasProperty(u, ExitTypeId);
var CauseTypeId = "~effect/Cause";
var CauseReasonTypeId = "~effect/Cause/Reason";
var isCause = (self2) => hasProperty(self2, CauseTypeId);
var CauseImpl = class {
  [CauseTypeId];
  reasons;
  constructor(failures) {
    this[CauseTypeId] = CauseTypeId;
    this.reasons = failures;
  }
  pipe() {
    return pipeArguments(this, arguments);
  }
  toJSON() {
    return {
      _id: "Cause",
      failures: this.reasons.map((f) => f.toJSON())
    };
  }
  toString() {
    return `Cause(${format(this.reasons)})`;
  }
  [NodeInspectSymbol]() {
    return this.toJSON();
  }
  [symbol2](that) {
    return isCause(that) && this.reasons.length === that.reasons.length && this.reasons.every((e, i) => equals(e, that.reasons[i]));
  }
  [symbol]() {
    return array(this.reasons);
  }
};
var annotationsMap = /* @__PURE__ */ new WeakMap();
var ReasonBase = class {
  [CauseReasonTypeId];
  annotations;
  _tag;
  constructor(_tag, annotations, originalError) {
    this[CauseReasonTypeId] = CauseReasonTypeId;
    this._tag = _tag;
    if (annotations !== constEmptyAnnotations && typeof originalError === "object" && originalError !== null && annotations.size > 0) {
      const prevAnnotations = annotationsMap.get(originalError);
      if (prevAnnotations) {
        annotations = new Map([...prevAnnotations, ...annotations]);
      }
      annotationsMap.set(originalError, annotations);
    }
    this.annotations = annotations;
  }
  annotate(annotations, options) {
    if (annotations.mapUnsafe.size === 0) return this;
    const newAnnotations = new Map(this.annotations);
    annotations.mapUnsafe.forEach((value, key) => {
      if (options?.overwrite !== true && newAnnotations.has(key)) return;
      newAnnotations.set(key, value);
    });
    const self2 = Object.assign(Object.create(Object.getPrototypeOf(this)), this);
    self2.annotations = newAnnotations;
    return self2;
  }
  pipe() {
    return pipeArguments(this, arguments);
  }
  toString() {
    return format(this);
  }
  [NodeInspectSymbol]() {
    return this.toString();
  }
};
var constEmptyAnnotations = /* @__PURE__ */ new Map();
var Fail = class extends ReasonBase {
  error;
  constructor(error, annotations = constEmptyAnnotations) {
    super("Fail", annotations, error);
    this.error = error;
  }
  toString() {
    return `Fail(${format(this.error)})`;
  }
  toJSON() {
    return {
      _tag: "Fail",
      error: this.error
    };
  }
  [symbol2](that) {
    return isFailReason(that) && equals(this.error, that.error) && equals(this.annotations, that.annotations);
  }
  [symbol]() {
    return combine(string(this._tag))(combine(hash(this.error))(hash(this.annotations)));
  }
};
var causeFromReasons = (reasons) => new CauseImpl(reasons);
var causeEmpty = /* @__PURE__ */ new CauseImpl([]);
var causeFail = (error) => new CauseImpl([new Fail(error)]);
var Die = class extends ReasonBase {
  defect;
  constructor(defect, annotations = constEmptyAnnotations) {
    super("Die", annotations, defect);
    this.defect = defect;
  }
  toString() {
    return `Die(${format(this.defect)})`;
  }
  toJSON() {
    return {
      _tag: "Die",
      defect: this.defect
    };
  }
  [symbol2](that) {
    return isDieReason(that) && equals(this.defect, that.defect) && equals(this.annotations, that.annotations);
  }
  [symbol]() {
    return combine(string(this._tag))(combine(hash(this.defect))(hash(this.annotations)));
  }
};
var causeDie = (defect) => new CauseImpl([new Die(defect)]);
var causeAnnotate = /* @__PURE__ */ dual((args2) => isCause(args2[0]), (self2, annotations, options) => {
  if (annotations.mapUnsafe.size === 0) return self2;
  return new CauseImpl(self2.reasons.map((f) => f.annotate(annotations, options)));
});
var isFailReason = (self2) => self2._tag === "Fail";
var isDieReason = (self2) => self2._tag === "Die";
var isInterruptReason = (self2) => self2._tag === "Interrupt";
function defaultEvaluate(_fiber) {
  return exitDie(`Effect.evaluate: Not implemented`);
}
var makePrimitiveProto = (options) => ({
  ...EffectProto,
  [identifier]: options.op,
  [evaluate]: options[evaluate] ?? defaultEvaluate,
  [contA]: options[contA],
  [contE]: options[contE],
  [contAll]: options[contAll]
});
var makePrimitive = (options) => {
  const Proto2 = makePrimitiveProto(options);
  return function() {
    const self2 = Object.create(Proto2);
    self2[args] = options.single === false ? arguments : arguments[0];
    return self2;
  };
};
var makeExit = (options) => {
  const Proto2 = {
    ...makePrimitiveProto(options),
    [ExitTypeId]: ExitTypeId,
    _tag: options.op,
    get [options.prop]() {
      return this[args];
    },
    toString() {
      return `${options.op}(${format(this[args])})`;
    },
    toJSON() {
      return {
        _id: "Exit",
        _tag: options.op,
        [options.prop]: this[args]
      };
    },
    [symbol2](that) {
      return isExit(that) && that._tag === this._tag && equals(this[args], that[args]);
    },
    [symbol]() {
      return combine(string(options.op), hash(this[args]));
    }
  };
  return function(value) {
    const self2 = Object.create(Proto2);
    self2[args] = value;
    return self2;
  };
};
var exitSucceed = /* @__PURE__ */ makeExit({
  op: "Success",
  prop: "value",
  [evaluate](fiber2) {
    const cont = fiber2.getCont(contA);
    return cont ? cont[contA](this[args], fiber2, this) : fiber2.yieldWith(this);
  }
});
var StackTraceKey = {
  key: "effect/Cause/StackTrace"
};
var InterruptorStackTrace = {
  key: "effect/Cause/InterruptorStackTrace"
};
var exitFailCause = /* @__PURE__ */ makeExit({
  op: "Failure",
  prop: "cause",
  [evaluate](fiber2) {
    let cause = this[args];
    let annotated = false;
    if (fiber2.currentStackFrame) {
      cause = causeAnnotate(cause, {
        mapUnsafe: /* @__PURE__ */ new Map([[StackTraceKey.key, fiber2.currentStackFrame]])
      });
      annotated = true;
    }
    let cont = fiber2.getCont(contE);
    while (fiber2.interruptible && fiber2._interruptedCause && cont) {
      cont = fiber2.getCont(contE);
    }
    return cont ? cont[contE](cause, fiber2, annotated ? void 0 : this) : fiber2.yieldWith(annotated ? this : exitFailCause(cause));
  }
});
var exitFail = (e) => exitFailCause(causeFail(e));
var exitDie = (defect) => exitFailCause(causeDie(defect));
var withFiber = /* @__PURE__ */ makePrimitive({
  op: "WithFiber",
  [evaluate](fiber2) {
    return this[args](fiber2);
  }
});
var YieldableError = /* @__PURE__ */ (function() {
  class YieldableError2 extends globalThis.Error {
  }
  const proto = /* @__PURE__ */ makePrimitiveProto({
    op: "YieldableError",
    [evaluate]() {
      return exitFail(this);
    }
  });
  delete proto.toString;
  Object.assign(YieldableError2.prototype, proto);
  return YieldableError2;
})();
var Error2 = /* @__PURE__ */ (function() {
  const plainArgsSymbol = /* @__PURE__ */ Symbol.for("effect/Data/Error/plainArgs");
  return class Base extends YieldableError {
    constructor(args2) {
      super(args2?.message, args2?.cause ? {
        cause: args2.cause
      } : void 0);
      if (args2) {
        Object.assign(this, args2);
        Object.defineProperty(this, plainArgsSymbol, {
          value: args2,
          enumerable: false
        });
      }
    }
    toJSON() {
      return {
        ...this[plainArgsSymbol],
        ...this
      };
    }
  };
})();
var TaggedError = (tag2) => {
  class Base3 extends Error2 {
    _tag = tag2;
  }
  ;
  Base3.prototype.name = tag2;
  return Base3;
};
var NoSuchElementErrorTypeId = "~effect/Cause/NoSuchElementError";
var NoSuchElementError = class extends (/* @__PURE__ */ TaggedError("NoSuchElementError")) {
  [NoSuchElementErrorTypeId] = NoSuchElementErrorTypeId;
  constructor(message) {
    super({
      message
    });
  }
};
var DoneTypeId = "~effect/Cause/Done";
var isDone = (u) => hasProperty(u, DoneTypeId);
var DoneVoid = {
  [DoneTypeId]: DoneTypeId,
  _tag: "Done",
  value: void 0
};
var Done = (value) => {
  if (value === void 0) return DoneVoid;
  return {
    [DoneTypeId]: DoneTypeId,
    _tag: "Done",
    value
  };
};
var doneVoid = /* @__PURE__ */ exitFail(DoneVoid);
var done = (value) => {
  if (value === void 0) return doneVoid;
  return exitFail(Done(value));
};

// node_modules/effect/dist/Effectable.js
var Prototype2 = (options) => makePrimitiveProto({
  op: options.label,
  [evaluate]: options.evaluate
});

// node_modules/effect/dist/internal/stackTraceLimit.js
var ObjectGetOwnPropertyDescriptor = Object.getOwnPropertyDescriptor;
var ObjectPrototypeHasOwnProperty = Object.prototype.hasOwnProperty;
var ObjectIsExtensible = Object.isExtensible;
var isStackTraceLimitWritable = () => {
  const desc = ObjectGetOwnPropertyDescriptor(Error, "stackTraceLimit");
  if (desc === void 0) {
    return ObjectIsExtensible(Error);
  }
  return ObjectPrototypeHasOwnProperty.call(desc, "writable") ? desc.writable === true : desc.set !== void 0;
};
var canWriteStackTraceLimit = /* @__PURE__ */ isStackTraceLimitWritable();
var getStackTraceLimit = () => Error.stackTraceLimit;
var setStackTraceLimit = (value) => {
  if (canWriteStackTraceLimit) {
    ;
    Error.stackTraceLimit = value;
  }
};

// node_modules/effect/dist/internal/option.js
var TypeId = "~effect/data/Option";
var CommonProto = {
  [TypeId]: {
    _A: (_) => _
  },
  ...PipeInspectableProto,
  [Symbol.iterator]() {
    return new SingleShotGen(this);
  }
};
var SomeProto = /* @__PURE__ */ Object.defineProperty(/* @__PURE__ */ Object.assign(/* @__PURE__ */ Object.create(CommonProto), {
  _tag: "Some",
  _op: "Some",
  [symbol2](that) {
    return isOption(that) && isSome(that) && equals(this.value, that.value);
  },
  [symbol]() {
    return combine(hash(this._tag))(hash(this.value));
  },
  toString() {
    return `some(${format(this.value)})`;
  },
  toJSON() {
    return {
      _id: "Option",
      _tag: this._tag,
      value: toJson(this.value)
    };
  }
}), "valueOrUndefined", {
  get() {
    return this.value;
  }
});
var NoneHash = /* @__PURE__ */ hash("None");
var NoneProto = /* @__PURE__ */ Object.assign(/* @__PURE__ */ Object.create(CommonProto), {
  _tag: "None",
  _op: "None",
  valueOrUndefined: void 0,
  [symbol2](that) {
    return isOption(that) && isNone(that);
  },
  [symbol]() {
    return NoneHash;
  },
  toString() {
    return `none()`;
  },
  toJSON() {
    return {
      _id: "Option",
      _tag: this._tag
    };
  }
});
var isOption = (input) => hasProperty(input, TypeId);
var isNone = (fa2) => fa2._tag === "None";
var isSome = (fa2) => fa2._tag === "Some";
var none = /* @__PURE__ */ Object.create(NoneProto);
var some = (value) => {
  const a = Object.create(SomeProto);
  a.value = value;
  return a;
};

// node_modules/effect/dist/internal/result.js
var TypeId2 = "~effect/data/Result";
var CommonProto2 = {
  [TypeId2]: {
    /* v8 ignore next 2 */
    _A: (_) => _,
    _E: (_) => _
  },
  ...PipeInspectableProto,
  [Symbol.iterator]() {
    return new SingleShotGen(this);
  }
};
var SuccessProto = /* @__PURE__ */ Object.assign(/* @__PURE__ */ Object.create(CommonProto2), {
  _tag: "Success",
  _op: "Success",
  [symbol2](that) {
    return isResult(that) && isSuccess(that) && equals(this.success, that.success);
  },
  [symbol]() {
    return combine(hash(this._tag))(hash(this.success));
  },
  toString() {
    return `success(${format(this.success)})`;
  },
  toJSON() {
    return {
      _id: "Result",
      _tag: this._tag,
      value: toJson(this.success)
    };
  }
});
var FailureProto = /* @__PURE__ */ Object.assign(/* @__PURE__ */ Object.create(CommonProto2), {
  _tag: "Failure",
  _op: "Failure",
  [symbol2](that) {
    return isResult(that) && isFailure(that) && equals(this.failure, that.failure);
  },
  [symbol]() {
    return combine(hash(this._tag))(hash(this.failure));
  },
  toString() {
    return `failure(${format(this.failure)})`;
  },
  toJSON() {
    return {
      _id: "Result",
      _tag: this._tag,
      failure: toJson(this.failure)
    };
  }
});
var isResult = (input) => hasProperty(input, TypeId2);
var isFailure = (result2) => result2._tag === "Failure";
var isSuccess = (result2) => result2._tag === "Success";
var fail = (failure) => {
  const a = Object.create(FailureProto);
  a.failure = failure;
  return a;
};
var succeed = (success) => {
  const a = Object.create(SuccessProto);
  a.success = success;
  return a;
};

// node_modules/effect/dist/Order.js
function make(compare) {
  return (self2, that) => self2 === that ? 0 : compare(self2, that);
}
var Number2 = /* @__PURE__ */ make((self2, that) => {
  if (globalThis.Number.isNaN(self2) && globalThis.Number.isNaN(that)) return 0;
  if (globalThis.Number.isNaN(self2)) return -1;
  if (globalThis.Number.isNaN(that)) return 1;
  return self2 < that ? -1 : 1;
});
var mapInput = /* @__PURE__ */ dual(2, (self2, f) => make((b1, b2) => self2(f(b1), f(b2))));
var Date2 = /* @__PURE__ */ mapInput(Number2, (date) => date.getTime());
var isGreaterThan = (O) => dual(2, (self2, that) => O(self2, that) === 1);

// node_modules/effect/dist/Option.js
var none2 = () => none;
var some2 = some;
var isNone2 = isNone;
var isSome2 = isSome;
var match = /* @__PURE__ */ dual(2, (self2, {
  onNone,
  onSome: onSome2
}) => isNone2(self2) ? onNone() : onSome2(self2.value));
var getOrElse = /* @__PURE__ */ dual(2, (self2, onNone) => isNone2(self2) ? onNone() : self2.value);
var fromNullishOr = (a) => a == null ? none2() : some2(a);
var getOrUndefined = /* @__PURE__ */ getOrElse(constUndefined);
var map = /* @__PURE__ */ dual(2, (self2, f) => isNone2(self2) ? none2() : some2(f(self2.value)));
var filter = /* @__PURE__ */ dual(2, (self2, predicate) => isNone2(self2) ? none2() : predicate(self2.value) ? some2(self2.value) : none2());

// node_modules/effect/dist/Context.js
var ServiceTypeId = "~effect/Context/Service";
var Service = function() {
  const prevLimit = getStackTraceLimit();
  setStackTraceLimit(2);
  const err = new Error();
  setStackTraceLimit(prevLimit);
  function KeyClass() {
  }
  const self2 = KeyClass;
  Object.setPrototypeOf(self2, ServiceProto);
  Object.defineProperty(self2, "stack", {
    get() {
      return err.stack;
    }
  });
  if (arguments.length > 0) {
    self2.key = arguments[0];
    if (arguments[1]?.defaultValue) {
      self2[ReferenceTypeId] = ReferenceTypeId;
      self2.defaultValue = arguments[1].defaultValue;
    }
    return self2;
  }
  return function(key, options) {
    self2.key = key;
    if (options?.make) {
      ;
      self2.make = options.make;
    }
    return self2;
  };
};
var ServiceProto = {
  [ServiceTypeId]: ServiceTypeId,
  .../* @__PURE__ */ Prototype2({
    label: "Service",
    evaluate(fiber2) {
      return exitSucceed(get(fiber2.context, this));
    }
  }),
  toJSON() {
    return {
      _id: "Service",
      key: this.key,
      stack: this.stack
    };
  },
  of(self2) {
    return self2;
  },
  context(self2) {
    return make2(this, self2);
  },
  use(f) {
    return withFiber((fiber2) => f(get(fiber2.context, this)));
  },
  useSync(f) {
    return withFiber((fiber2) => exitSucceed(f(get(fiber2.context, this))));
  }
};
var ReferenceTypeId = "~effect/Context/Reference";
var TypeId3 = "~effect/Context";
var makeUnsafe = (mapUnsafe) => {
  const self2 = Object.create(Proto);
  self2.mapUnsafe = mapUnsafe;
  self2.mutable = false;
  return self2;
};
var Proto = {
  ...PipeInspectableProto,
  [TypeId3]: {
    _Services: (_) => _
  },
  toJSON() {
    return {
      _id: "Context",
      services: Array.from(this.mapUnsafe).map(([key, value]) => ({
        key,
        value
      }))
    };
  },
  [symbol2](that) {
    if (!isContext(that) || this.mapUnsafe.size !== that.mapUnsafe.size) return false;
    for (const k of this.mapUnsafe.keys()) {
      if (!that.mapUnsafe.has(k) || !equals(this.mapUnsafe.get(k), that.mapUnsafe.get(k))) {
        return false;
      }
    }
    return true;
  },
  [symbol]() {
    return number(this.mapUnsafe.size);
  }
};
var isContext = (u) => hasProperty(u, TypeId3);
var isReference = (u) => hasProperty(u, ReferenceTypeId);
var empty = () => emptyContext2;
var emptyContext2 = /* @__PURE__ */ makeUnsafe(/* @__PURE__ */ new Map());
var make2 = (key, service3) => makeUnsafe(/* @__PURE__ */ new Map([[key.key, service3]]));
var add = /* @__PURE__ */ dual(3, (self2, key, service3) => withMapUnsafe(self2, (map8) => {
  map8.set(key.key, service3);
}));
var getOrElse2 = /* @__PURE__ */ dual(3, (self2, key, orElse) => {
  if (self2.mapUnsafe.has(key.key)) {
    return self2.mapUnsafe.get(key.key);
  }
  return isReference(key) ? getDefaultValue(key) : orElse();
});
var getUnsafe = /* @__PURE__ */ dual(2, (self2, service3) => {
  if (!self2.mapUnsafe.has(service3.key)) {
    if (ReferenceTypeId in service3) return getDefaultValue(service3);
    throw serviceNotFoundError(service3);
  }
  return self2.mapUnsafe.get(service3.key);
});
var get = getUnsafe;
var getReferenceUnsafe = (self2, service3) => {
  if (!self2.mapUnsafe.has(service3.key)) {
    return getDefaultValue(service3);
  }
  return self2.mapUnsafe.get(service3.key);
};
var defaultValueCacheKey = "~effect/Context/defaultValue";
var getDefaultValue = (ref) => {
  if (defaultValueCacheKey in ref) {
    return ref[defaultValueCacheKey];
  }
  return ref[defaultValueCacheKey] = ref.defaultValue();
};
var serviceNotFoundError = (service3) => {
  const error = new Error(`Service not found${service3.key ? `: ${String(service3.key)}` : ""}`);
  if (service3.stack) {
    const lines = service3.stack.split("\n");
    if (lines.length > 2) {
      const afterAt = lines[2].match(/at (.*)/);
      if (afterAt) {
        error.message = error.message + ` (defined at ${afterAt[1]})`;
      }
    }
  }
  if (error.stack) {
    const lines = error.stack.split("\n");
    lines.splice(1, 3);
    error.stack = lines.join("\n");
  }
  return error;
};
var getOption = /* @__PURE__ */ dual(2, (self2, service3) => {
  if (self2.mapUnsafe.has(service3.key)) {
    return some2(self2.mapUnsafe.get(service3.key));
  }
  return isReference(service3) ? some2(getDefaultValue(service3)) : none2();
});
var merge = /* @__PURE__ */ dual(2, (self2, that) => {
  if (self2.mapUnsafe.size === 0) return that;
  if (that.mapUnsafe.size === 0) return self2;
  return withMapUnsafe(self2, (map8) => {
    that.mapUnsafe.forEach((value, key) => map8.set(key, value));
  });
});
var mergeAll = (...ctxs) => {
  const map8 = /* @__PURE__ */ new Map();
  for (let i = 0; i < ctxs.length; i++) {
    ctxs[i].mapUnsafe.forEach((value, key) => {
      map8.set(key, value);
    });
  }
  return makeUnsafe(map8);
};
var mutate = /* @__PURE__ */ dual(2, (self2, f) => {
  const next = makeUnsafe(new Map(self2.mapUnsafe));
  next.mutable = true;
  const result2 = f(next);
  result2.mutable = false;
  return result2;
});
var withMapUnsafe = (self2, f) => {
  if (self2.mutable) {
    f(self2.mapUnsafe);
    return self2;
  }
  const map8 = new Map(self2.mapUnsafe);
  f(map8);
  return makeUnsafe(map8);
};
var Reference = Service;

// node_modules/effect/dist/Duration.js
var TypeId4 = "~effect/time/Duration";
var bigint0 = /* @__PURE__ */ BigInt(0);
var bigint1 = /* @__PURE__ */ BigInt(1);
var bigint1e3 = /* @__PURE__ */ BigInt(1e3);
var roundTiesAwayFromZero = (input) => BigInt(input < 0 ? Math.ceil(input - 0.5) : Math.floor(input + 0.5));
var roundMillisToNanos = (millis2) => roundTiesAwayFromZero(millis2 * 1e6);
var parseNanos = (input, scale) => input.includes(".") ? roundTiesAwayFromZero(Number(input) * Number(scale)) : BigInt(input) * scale;
var DURATION_REGEXP = /^(-?\d+(?:\.\d+)?)\s+(nanos?|micros?|millis?|seconds?|minutes?|hours?|days?|weeks?)$/;
var fromInputUnsafe = (input) => {
  switch (typeof input) {
    case "number":
      return millis(input);
    case "bigint":
      return nanos(input);
    case "string": {
      if (input === "Infinity") {
        return infinity;
      }
      if (input === "-Infinity") {
        return negativeInfinity;
      }
      const match6 = DURATION_REGEXP.exec(input);
      if (!match6) break;
      const [_, valueStr, unit] = match6;
      if (unit === "nano" || unit === "nanos") {
        return nanos(parseNanos(valueStr, bigint1));
      }
      if (unit === "micro" || unit === "micros") {
        return nanos(parseNanos(valueStr, bigint1e3));
      }
      const value = Number(valueStr);
      switch (unit) {
        case "milli":
        case "millis":
          return millis(value);
        case "second":
        case "seconds":
          return seconds(value);
        case "minute":
        case "minutes":
          return minutes(value);
        case "hour":
        case "hours":
          return hours(value);
        case "day":
        case "days":
          return days(value);
        case "week":
        case "weeks":
          return weeks(value);
      }
      break;
    }
    case "object": {
      if (input === null) break;
      if (TypeId4 in input) return input;
      if (Array.isArray(input)) {
        if (input.length !== 2 || !input.every(isNumber)) {
          return invalid(input);
        }
        if (Number.isNaN(input[0]) || Number.isNaN(input[1])) {
          return zero;
        }
        if (input[0] === -Infinity || input[1] === -Infinity) {
          return negativeInfinity;
        }
        if (input[0] === Infinity || input[1] === Infinity) {
          return infinity;
        }
        return make3(roundTiesAwayFromZero(input[0] * 1e9 + input[1]));
      }
      const obj = input;
      let millis2 = 0;
      if (obj.weeks) millis2 += obj.weeks * 6048e5;
      if (obj.days) millis2 += obj.days * 864e5;
      if (obj.hours) millis2 += obj.hours * 36e5;
      if (obj.minutes) millis2 += obj.minutes * 6e4;
      if (obj.seconds) millis2 += obj.seconds * 1e3;
      if (obj.milliseconds) millis2 += obj.milliseconds;
      if (!obj.microseconds && !obj.nanoseconds) return make3(millis2);
      return make3(roundTiesAwayFromZero(millis2 * 1e6 + (obj.microseconds ?? 0) * 1e3 + (obj.nanoseconds ?? 0)));
    }
  }
  return invalid(input);
};
var invalid = (input) => {
  throw new Error(`Invalid Input: ${input}`);
};
var zeroDurationValue = {
  _tag: "Millis",
  millis: 0
};
var infinityDurationValue = {
  _tag: "Infinity"
};
var negativeInfinityDurationValue = {
  _tag: "NegativeInfinity"
};
var DurationProto = {
  [TypeId4]: TypeId4,
  [symbol]() {
    return structure(this.value);
  },
  [symbol2](that) {
    return isDuration(that) && equals2(this, that);
  },
  toString() {
    switch (this.value._tag) {
      case "Infinity":
        return "Infinity";
      case "NegativeInfinity":
        return "-Infinity";
      case "Nanos":
        return `${this.value.nanos} nanos`;
      case "Millis":
        return `${this.value.millis} millis`;
    }
  },
  toJSON() {
    switch (this.value._tag) {
      case "Millis":
        return {
          _id: "Duration",
          _tag: "Millis",
          millis: this.value.millis
        };
      case "Nanos":
        return {
          _id: "Duration",
          _tag: "Nanos",
          nanos: String(this.value.nanos)
        };
      case "Infinity":
        return {
          _id: "Duration",
          _tag: "Infinity"
        };
      case "NegativeInfinity":
        return {
          _id: "Duration",
          _tag: "NegativeInfinity"
        };
    }
  },
  [NodeInspectSymbol]() {
    return this.toJSON();
  },
  pipe() {
    return pipeArguments(this, arguments);
  }
};
var make3 = (input) => {
  const duration = Object.create(DurationProto);
  if (typeof input === "number") {
    if (isNaN(input) || input === 0 || Object.is(input, -0)) {
      duration.value = zeroDurationValue;
    } else if (!Number.isFinite(input)) {
      duration.value = input > 0 ? infinityDurationValue : negativeInfinityDurationValue;
    } else if (!Number.isInteger(input)) {
      duration.value = {
        _tag: "Nanos",
        nanos: roundMillisToNanos(input)
      };
    } else {
      duration.value = {
        _tag: "Millis",
        millis: input
      };
    }
  } else if (input === bigint0) {
    duration.value = zeroDurationValue;
  } else {
    duration.value = {
      _tag: "Nanos",
      nanos: input
    };
  }
  return duration;
};
var isDuration = (u) => hasProperty(u, TypeId4);
var zero = /* @__PURE__ */ make3(0);
var infinity = /* @__PURE__ */ make3(Infinity);
var negativeInfinity = /* @__PURE__ */ make3(-Infinity);
var nanos = (nanos2) => make3(nanos2);
var millis = (millis2) => make3(millis2);
var seconds = (seconds2) => make3(seconds2 * 1e3);
var minutes = (minutes2) => make3(minutes2 * 6e4);
var hours = (hours2) => make3(hours2 * 36e5);
var days = (days2) => make3(days2 * 864e5);
var weeks = (weeks2) => make3(weeks2 * 6048e5);
var toMillis = (self2) => match2(fromInputUnsafe(self2), {
  onMillis: identity,
  onNanos: (nanos2) => Number(nanos2) / 1e6,
  onInfinity: () => Infinity,
  onNegativeInfinity: () => -Infinity
});
var toNanosUnsafe = (input) => {
  const self2 = fromInputUnsafe(input);
  switch (self2.value._tag) {
    case "Infinity":
    case "NegativeInfinity":
      throw new Error("Cannot convert infinite duration to nanos");
    case "Nanos":
      return self2.value.nanos;
    case "Millis":
      return roundMillisToNanos(self2.value.millis);
  }
};
var match2 = /* @__PURE__ */ dual(2, (self2, options) => {
  switch (self2.value._tag) {
    case "Millis":
      return options.onMillis(self2.value.millis);
    case "Nanos":
      return options.onNanos(self2.value.nanos);
    case "Infinity":
      return options.onInfinity();
    case "NegativeInfinity":
      return (options.onNegativeInfinity ?? options.onInfinity)();
  }
});
var matchPair = /* @__PURE__ */ dual(3, (self2, that, options) => {
  if (self2.value._tag === "Infinity" || self2.value._tag === "NegativeInfinity" || that.value._tag === "Infinity" || that.value._tag === "NegativeInfinity") return options.onInfinity(self2, that);
  if (self2.value._tag === "Millis") {
    return that.value._tag === "Millis" ? options.onMillis(self2.value.millis, that.value.millis) : options.onNanos(toNanosUnsafe(self2), that.value.nanos);
  } else {
    return options.onNanos(self2.value.nanos, toNanosUnsafe(that));
  }
});
var Equivalence = (self2, that) => matchPair(self2, that, {
  onMillis: (self3, that2) => self3 === that2,
  onNanos: (self3, that2) => self3 === that2,
  onInfinity: (self3, that2) => self3.value._tag === that2.value._tag
});
var equals2 = /* @__PURE__ */ dual(2, (self2, that) => Equivalence(self2, that));

// node_modules/effect/dist/internal/array.js
var isArrayNonEmpty = (self2) => self2.length > 0;

// node_modules/effect/dist/Result.js
var succeed2 = succeed;
var fail2 = fail;
var isFailure2 = isFailure;
var match3 = /* @__PURE__ */ dual(2, (self2, {
  onFailure,
  onSuccess
}) => isFailure2(self2) ? onFailure(self2.failure) : onSuccess(self2.success));

// node_modules/effect/dist/Array.js
var Array2 = globalThis.Array;
var fromIterable = (collection) => Array2.isArray(collection) ? collection : Array2.from(collection);
var append = /* @__PURE__ */ dual(2, (self2, last) => [...self2, last]);
var appendAll = /* @__PURE__ */ dual(2, (self2, that) => fromIterable(self2).concat(fromIterable(that)));
var isArray = Array2.isArray;
var isArrayEmpty = (self2) => self2.length === 0;
var isReadonlyArrayEmpty = isArrayEmpty;
var isArrayNonEmpty2 = isArrayNonEmpty;
var isReadonlyArrayNonEmpty = isArrayNonEmpty;
function isOutOfBounds(i, as3) {
  return i < 0 || i >= as3.length;
}
var getUnsafe2 = /* @__PURE__ */ dual(2, (self2, index) => {
  const i = Math.floor(index);
  if (isOutOfBounds(i, self2)) {
    throw new Error(`Index out of bounds: ${i}`);
  }
  return self2[i];
});
var headNonEmpty = /* @__PURE__ */ getUnsafe2(0);
var tailNonEmpty = (self2) => self2.slice(1);
var sort = /* @__PURE__ */ dual(2, (self2, O) => {
  const out = Array2.from(self2);
  out.sort(O);
  return out;
});
var unionWith = /* @__PURE__ */ dual(3, (self2, that, isEquivalent) => {
  const a = fromIterable(self2);
  const b = fromIterable(that);
  if (isReadonlyArrayNonEmpty(a)) {
    if (isReadonlyArrayNonEmpty(b)) {
      const dedupe = dedupeWith(isEquivalent);
      return dedupe(appendAll(a, b));
    }
    return a;
  }
  return b;
});
var union = /* @__PURE__ */ dual(2, (self2, that) => unionWith(self2, that, asEquivalence()));
var empty2 = () => [];
var of = (a) => [a];
var map2 = /* @__PURE__ */ dual(2, (self2, f) => self2.map(f));
var flatMap = /* @__PURE__ */ dual(2, (self2, f) => {
  if (isReadonlyArrayEmpty(self2)) {
    return [];
  }
  const out = [];
  for (let i = 0; i < self2.length; i++) {
    const inner = f(self2[i], i);
    for (let j = 0; j < inner.length; j++) {
      out.push(inner[j]);
    }
  }
  return out;
});
var fromNullishOr2 = (a) => a == null ? empty2() : [a];
var flatMapNullishOr = /* @__PURE__ */ dual(2, (self2, f) => flatMap(self2, (a) => fromNullishOr2(f(a))));
var dedupeWith = /* @__PURE__ */ dual(2, (self2, isEquivalent) => {
  const input = fromIterable(self2);
  if (isReadonlyArrayNonEmpty(input)) {
    const out = [headNonEmpty(input)];
    const rest = tailNonEmpty(input);
    for (const r of rest) {
      if (out.every((a) => !isEquivalent(r, a))) {
        out.push(r);
      }
    }
    return out;
  }
  return [];
});

// node_modules/effect/dist/Filter.js
var composePassthrough = /* @__PURE__ */ dual(2, (left, right) => (input) => {
  const leftOut = left(input);
  if (isFailure2(leftOut)) return fail2(input);
  const rightOut = right(leftOut.success);
  if (isFailure2(rightOut)) return fail2(input);
  return rightOut;
});

// node_modules/effect/dist/Scheduler.js
var Scheduler = /* @__PURE__ */ Reference("effect/Scheduler", {
  defaultValue: () => new MixedScheduler()
});
var setImmediate = "setImmediate" in globalThis ? (f) => {
  const timer = globalThis.setImmediate(f);
  return () => globalThis.clearImmediate(timer);
} : (f) => {
  const timer = setTimeout(f, 0);
  return () => clearTimeout(timer);
};
var PriorityBuckets = class {
  buckets = [];
  scheduleTask(task, priority) {
    const buckets = this.buckets;
    const len = buckets.length;
    let bucket;
    let index = 0;
    for (; index < len; index++) {
      if (buckets[index][0] > priority) break;
      bucket = buckets[index];
    }
    if (bucket && bucket[0] === priority) {
      bucket[1].push(task);
    } else if (index === len) {
      buckets.push([priority, [task]]);
    } else {
      buckets.splice(index, 0, [priority, [task]]);
    }
  }
  drain() {
    const buckets = this.buckets;
    this.buckets = [];
    return buckets;
  }
};
var MixedScheduler = class {
  executionMode;
  setImmediate;
  constructor(executionMode = "async", setImmediateFn = setImmediate) {
    this.executionMode = executionMode;
    this.setImmediate = setImmediateFn;
  }
  /**
   * Returns whether the fiber has reached its operation budget and should yield.
   *
   * **When to use**
   *
   * Use to decide whether a fiber should yield after consuming its current
   * operation budget.
   *
   * @since 2.0.0
   */
  shouldYield(fiber2) {
    return fiber2.currentOpCount >= fiber2.maxOpsBeforeYield;
  }
  /**
   * Creates a dispatcher that schedules work through this scheduler.
   *
   * **When to use**
   *
   * Use when you need a standalone dispatcher from a scheduler instance, for
   * example in tests that enqueue tasks and then flush them deterministically.
   *
   * @since 4.0.0
   */
  makeDispatcher() {
    return new MixedSchedulerDispatcher(this.setImmediate);
  }
};
var MixedSchedulerDispatcher = class {
  tasks = /* @__PURE__ */ new PriorityBuckets();
  running = void 0;
  setImmediate;
  constructor(setImmediateFn = setImmediate) {
    this.setImmediate = setImmediateFn;
  }
  /**
   * @since 2.0.0
   */
  scheduleTask(task, priority) {
    this.tasks.scheduleTask(task, priority);
    if (this.running === void 0) {
      this.running = this.setImmediate(this.afterScheduled);
    }
  }
  /**
   * @since 2.0.0
   */
  afterScheduled = () => {
    this.running = void 0;
    this.runTasks();
  };
  /**
   * @since 2.0.0
   */
  runTasks() {
    const buckets = this.tasks.drain();
    for (let i = 0; i < buckets.length; i++) {
      const toRun = buckets[i][1];
      for (let j = 0; j < toRun.length; j++) {
        toRun[j]();
      }
    }
  }
  /**
   * @since 2.0.0
   */
  flush() {
    while (this.tasks.buckets.length > 0) {
      if (this.running !== void 0) {
        this.running();
        this.running = void 0;
      }
      this.runTasks();
    }
  }
};
var MaxOpsBeforeYield = /* @__PURE__ */ Reference("effect/Scheduler/MaxOpsBeforeYield", {
  defaultValue: () => 2048
});
var PreventSchedulerYield = /* @__PURE__ */ Reference("effect/Scheduler/PreventSchedulerYield", {
  defaultValue: () => false
});

// node_modules/effect/dist/Tracer.js
var ParentSpanKey = "effect/Tracer/ParentSpan";
var ParentSpan = class extends (/* @__PURE__ */ Service()(ParentSpanKey)) {
};
var make4 = (options) => options;
var DisablePropagation = /* @__PURE__ */ Reference("effect/Tracer/DisablePropagation", {
  defaultValue: constFalse
});
var CurrentTraceLevel = /* @__PURE__ */ Reference("effect/Tracer/CurrentTraceLevel", {
  defaultValue: () => "Info"
});
var MinimumTraceLevel = /* @__PURE__ */ Reference("effect/Tracer/MinimumTraceLevel", {
  defaultValue: () => "All"
});
var TracerKey = "effect/Tracer";
var Tracer = /* @__PURE__ */ Reference(TracerKey, {
  defaultValue: () => make4({
    span: (options) => new NativeSpan(options)
  })
});
var NativeSpan = class {
  _tag = "Span";
  spanId;
  traceId = "native";
  sampled;
  name;
  parent;
  annotations;
  links;
  startTime;
  kind;
  status;
  attributes;
  events = [];
  constructor(options) {
    this.name = options.name;
    this.parent = options.parent;
    this.annotations = options.annotations;
    this.links = options.links;
    this.startTime = options.startTime;
    this.kind = options.kind;
    this.sampled = options.sampled;
    this.status = {
      _tag: "Started",
      startTime: options.startTime
    };
    this.attributes = /* @__PURE__ */ new Map();
    this.traceId = getOrUndefined(options.parent)?.traceId ?? randomHexString(32);
    this.spanId = randomHexString(16);
  }
  end(endTime, exit3) {
    this.status = {
      _tag: "Ended",
      endTime,
      exit: exit3,
      startTime: this.status.startTime
    };
  }
  attribute(key, value) {
    this.attributes.set(key, value);
  }
  event(name, startTime, attributes) {
    this.events.push([name, startTime, attributes ?? {}]);
  }
  addLinks(links) {
    this.links.push(...links);
  }
};
var randomHexString = /* @__PURE__ */ (function() {
  const characters = "abcdef0123456789";
  const charactersLength = characters.length;
  return function(length) {
    let result2 = "";
    for (let i = 0; i < length; i++) {
      result2 += characters.charAt(Math.floor(Math.random() * charactersLength));
    }
    return result2;
  };
})();

// node_modules/effect/dist/internal/metric.js
var FiberRuntimeMetricsKey = "effect/observability/Metric/FiberRuntimeMetricsKey";

// node_modules/effect/dist/internal/references.js
var CurrentConcurrency = /* @__PURE__ */ Reference("effect/References/CurrentConcurrency", {
  defaultValue: () => "unbounded"
});
var CurrentStackFrame = /* @__PURE__ */ Reference("effect/References/CurrentStackFrame", {
  defaultValue: constUndefined
});
var TracerEnabled = /* @__PURE__ */ Reference("effect/References/TracerEnabled", {
  defaultValue: constTrue
});
var TracerTimingEnabled = /* @__PURE__ */ Reference("effect/References/TracerTimingEnabled", {
  defaultValue: constTrue
});
var TracerSpanAnnotations = /* @__PURE__ */ Reference("effect/References/TracerSpanAnnotations", {
  defaultValue: () => ({})
});
var TracerSpanLinks = /* @__PURE__ */ Reference("effect/References/TracerSpanLinks", {
  defaultValue: () => []
});
var CurrentLogAnnotations = /* @__PURE__ */ Reference("effect/References/CurrentLogAnnotations", {
  defaultValue: () => ({})
});
var CurrentLogLevel = /* @__PURE__ */ Reference("effect/References/CurrentLogLevel", {
  defaultValue: () => "Info"
});
var MinimumLogLevel = /* @__PURE__ */ Reference("effect/References/MinimumLogLevel", {
  defaultValue: () => "Info"
});
var CurrentLogSpans = /* @__PURE__ */ Reference("effect/References/CurrentLogSpans", {
  defaultValue: () => []
});

// node_modules/effect/dist/internal/tracer.js
var addSpanStackTrace = (options) => {
  if (options?.captureStackTrace === false) {
    return options;
  } else if (options?.captureStackTrace !== void 0 && typeof options.captureStackTrace !== "boolean") {
    return options;
  }
  const limit = getStackTraceLimit();
  setStackTraceLimit(3);
  const traceError = new Error();
  setStackTraceLimit(limit);
  return {
    ...options,
    captureStackTrace: spanCleaner(() => traceError.stack)
  };
};
var makeStackCleaner = (line) => (stack) => {
  let cache;
  return () => {
    if (cache !== void 0) return cache;
    const trace = stack();
    if (!trace) return void 0;
    const lines = trace.split("\n");
    if (lines[line] !== void 0) {
      cache = lines[line].trim();
      return cache;
    }
  };
};
var spanCleaner = /* @__PURE__ */ makeStackCleaner(3);

// node_modules/effect/dist/internal/version.js
var version = "dev";

// node_modules/effect/dist/internal/effect.js
var Interrupt = class extends ReasonBase {
  fiberId;
  constructor(fiberId2, annotations = constEmptyAnnotations) {
    super("Interrupt", annotations, "Interrupted");
    this.fiberId = fiberId2;
  }
  toString() {
    return `Interrupt(${this.fiberId})`;
  }
  toJSON() {
    return {
      _tag: "Interrupt",
      fiberId: this.fiberId
    };
  }
  [symbol2](that) {
    return isInterruptReason(that) && this.fiberId === that.fiberId && this.annotations === that.annotations;
  }
  [symbol]() {
    return combine(string(`${this._tag}:${this.fiberId}`))(random(this.annotations));
  }
};
var causeInterrupt = (fiberId2) => new CauseImpl([new Interrupt(fiberId2)]);
var findError = (self2) => {
  for (let i = 0; i < self2.reasons.length; i++) {
    const reason = self2.reasons[i];
    if (reason._tag === "Fail") {
      return succeed2(reason.error);
    }
  }
  return fail2(self2);
};
var findDefect = (self2) => {
  const reason = self2.reasons.find(isDieReason);
  return reason ? succeed2(reason.defect) : fail2(self2);
};
var hasInterrupts = (self2) => self2.reasons.some(isInterruptReason);
var causeCombine = /* @__PURE__ */ dual(2, (self2, that) => {
  if (self2.reasons.length === 0) {
    return that;
  } else if (that.reasons.length === 0) {
    return self2;
  }
  const newCause = new CauseImpl(union(self2.reasons, that.reasons));
  return equals(self2, newCause) ? self2 : newCause;
});
var causeMap = /* @__PURE__ */ dual(2, (self2, f) => {
  let hasFail = false;
  const failures = self2.reasons.map((failure) => {
    if (isFailReason(failure)) {
      hasFail = true;
      return new Fail(f(failure.error));
    }
    return failure;
  });
  return hasFail ? causeFromReasons(failures) : self2;
});
var causePartition = (self2) => {
  const obj = {
    Fail: [],
    Die: [],
    Interrupt: []
  };
  for (let i = 0; i < self2.reasons.length; i++) {
    obj[self2.reasons[i]._tag].push(self2.reasons[i]);
  }
  return obj;
};
var causeSquash = (self2) => {
  const partitioned = causePartition(self2);
  if (partitioned.Fail.length > 0) {
    return partitioned.Fail[0].error;
  } else if (partitioned.Die.length > 0) {
    return partitioned.Die[0].defect;
  } else if (partitioned.Interrupt.length > 0) {
    return new globalThis.Error("All fibers interrupted without error");
  }
  return new globalThis.Error("Empty cause");
};
var causePrettyErrors = (self2, options) => {
  const errors = [];
  const interrupts = [];
  if (self2.reasons.length === 0) return errors;
  const prevStackLimit = getStackTraceLimit();
  setStackTraceLimit(1);
  for (const failure of self2.reasons) {
    if (failure._tag === "Interrupt") {
      interrupts.push(failure);
      continue;
    }
    errors.push(causePrettyError(failure._tag === "Die" ? failure.defect : failure.error, failure.annotations, options));
  }
  if (errors.length === 0) {
    const cause = new Error("The fiber was interrupted by:");
    cause.name = "InterruptCause";
    cause.stack = interruptCauseStack(cause, interrupts);
    const error = new globalThis.Error("All fibers interrupted without error", {
      cause
    });
    error.name = "InterruptError";
    error.stack = `${error.name}: ${error.message}`;
    errors.push(causePrettyError(error, interrupts[0].annotations, options));
  }
  setStackTraceLimit(prevStackLimit);
  return errors;
};
var causePrettyError = (original, annotations, options) => {
  const kind = typeof original;
  let error;
  if (original && kind === "object") {
    error = new globalThis.Error(causePrettyMessage(original), {
      cause: original.cause ? causePrettyError(original.cause) : void 0
    });
    if (typeof original.name === "string") {
      error.name = original.name;
    }
    if (typeof original.stack === "string") {
      error.stack = cleanErrorStack(original.stack, error, annotations);
    } else {
      const stack = `${error.name}: ${error.message}`;
      error.stack = annotations ? addStackAnnotations(stack, annotations) : stack;
    }
    if (options?.includeCauseInStack) {
      error.stack = renderPrettyError(error);
    }
    for (const key of Object.keys(original)) {
      if (!(key in error)) {
        ;
        error[key] = original[key];
      }
    }
  } else {
    error = new globalThis.Error(!original ? `Unknown error: ${original}` : kind === "string" ? original : formatJson(original));
  }
  return error;
};
var causePrettyMessage = (u) => {
  if (typeof u.message === "string") {
    return u.message;
  } else if (typeof u.toString === "function" && u.toString !== Object.prototype.toString && u.toString !== Array.prototype.toString) {
    try {
      return u.toString();
    } catch {
    }
  }
  return formatJson(u);
};
var locationRegExp = /\((.*)\)/g;
var cleanErrorStack = (stack, error, annotations) => {
  const message = `${error.name}: ${error.message}`;
  const lines = (stack.startsWith(message) ? stack.slice(message.length) : stack).split("\n");
  const out = [message];
  for (let i = 1; i < lines.length; i++) {
    if (/(?:Generator\.next|~effect\/Effect)/.test(lines[i])) {
      break;
    }
    out.push(lines[i]);
  }
  return annotations ? addStackAnnotations(out.join("\n"), annotations) : out.join("\n");
};
var addStackAnnotations = (stack, annotations) => {
  const frame = annotations?.get(StackTraceKey.key);
  if (frame) {
    stack = `${stack}
${currentStackTrace(frame)}`;
  }
  return stack;
};
var interruptCauseStack = (error, interrupts) => {
  const out = [`${error.name}: ${error.message}`];
  for (const current of interrupts) {
    const fiberId2 = current.fiberId !== void 0 ? `#${current.fiberId}` : "unknown";
    const frame = current.annotations.get(InterruptorStackTrace.key);
    out.push(`    at fiber (${fiberId2})`);
    if (frame) out.push(currentStackTrace(frame));
  }
  return out.join("\n");
};
var currentStackTrace = (frame) => {
  const out = [];
  let current = frame;
  let i = 0;
  while (current && i < 10) {
    const stack = current.stack();
    if (stack) {
      const locationMatchAll = stack.matchAll(locationRegExp);
      let match6 = false;
      for (const [, location] of locationMatchAll) {
        match6 = true;
        out.push(`    at ${current.name} (${location})`);
      }
      if (!match6) {
        out.push(`    at ${current.name} (${stack.replace(/^at /, "")})`);
      }
    } else {
      out.push(`    at ${current.name}`);
    }
    current = current.parent;
    i++;
  }
  return out.join("\n");
};
var causePretty = (cause) => causePrettyErrors(cause).map(renderPrettyError).join("\n");
var renderPrettyError = (e) => e.cause ? `${e.stack} {
${renderErrorCause(e.cause, "  ")}
}` : e.stack;
var renderErrorCause = (cause, prefix) => {
  const lines = cause.stack.split("\n");
  let stack = `${prefix}[cause]: ${lines[0]}`;
  for (let i = 1, len = lines.length; i < len; i++) {
    stack += `
${prefix}${lines[i]}`;
  }
  if (cause.cause) {
    stack += ` {
${renderErrorCause(cause.cause, `${prefix}  `)}
${prefix}}`;
  }
  return stack;
};
var FiberTypeId = `~effect/Fiber/${version}`;
var fiberVariance = {
  _A: identity,
  _E: identity
};
var fiberIdStore = {
  id: 0
};
var getCurrentFiber = () => globalThis[currentFiberTypeId];
var FiberImpl = class {
  constructor(context3, interruptible2 = true) {
    this[FiberTypeId] = fiberVariance;
    this.setContext(context3);
    this.id = ++fiberIdStore.id;
    this.currentOpCount = 0;
    this.currentLoopCount = 0;
    this.interruptible = interruptible2;
    this._stack = [];
    this._observers = [];
    this._exit = void 0;
    this._children = void 0;
    this._interruptedCause = void 0;
    this._yielded = void 0;
    this.runtimeMetrics?.recordFiberStart(this.context);
  }
  [FiberTypeId];
  id;
  interruptible;
  currentOpCount;
  currentLoopCount;
  _stack;
  _observers;
  _exit;
  _currentExit;
  _children;
  _interruptedCause;
  _yielded;
  // set in setContext
  context;
  currentScheduler;
  currentTracerContext;
  currentSpan;
  currentLogLevel;
  minimumLogLevel;
  currentStackFrame;
  runtimeMetrics;
  maxOpsBeforeYield;
  currentPreventYield;
  _dispatcher = void 0;
  get currentDispatcher() {
    return this._dispatcher ??= this.currentScheduler.makeDispatcher();
  }
  getRef(ref) {
    return getReferenceUnsafe(this.context, ref);
  }
  addObserver(cb2) {
    if (this._exit) {
      cb2(this._exit);
      return constVoid;
    }
    this._observers.push(cb2);
    return () => {
      const index = this._observers.indexOf(cb2);
      if (index >= 0) {
        this._observers.splice(index, 1);
      }
    };
  }
  interruptUnsafe(fiberId2, annotations) {
    if (this._exit) {
      return;
    }
    let cause = causeInterrupt(fiberId2);
    if (this.currentStackFrame) {
      cause = causeAnnotate(cause, make2(StackTraceKey, this.currentStackFrame));
    }
    if (annotations) {
      cause = causeAnnotate(cause, annotations);
    }
    this._interruptedCause = this._interruptedCause ? causeCombine(this._interruptedCause, cause) : cause;
    if (this.interruptible) {
      this.evaluate(failCause(this._interruptedCause));
    }
  }
  pollUnsafe() {
    return this._exit;
  }
  evaluate(effect2) {
    if (this._exit) {
      return;
    } else if (this._yielded !== void 0) {
      const yielded = this._yielded;
      this._yielded = void 0;
      yielded();
    }
    const exit3 = this.runLoop(effect2);
    if (exit3 === Yield) {
      return;
    }
    const interruptChildren = fiberMiddleware.interruptChildren && fiberMiddleware.interruptChildren(this);
    if (interruptChildren !== void 0) {
      return this.evaluate(flatMap2(interruptChildren, () => exit3));
    }
    this._exit = exit3;
    this.runtimeMetrics?.recordFiberEnd(this.context, this._exit);
    for (let i = 0; i < this._observers.length; i++) {
      this._observers[i](exit3);
    }
    this._observers.length = 0;
  }
  runLoop(effect2) {
    const prevFiber = globalThis[currentFiberTypeId];
    globalThis[currentFiberTypeId] = this;
    let yielding = false;
    let current = effect2;
    this.currentOpCount = 0;
    const currentLoop = ++this.currentLoopCount;
    try {
      while (true) {
        this.currentOpCount++;
        if (!yielding && !this.currentPreventYield && this.currentScheduler.shouldYield(this)) {
          yielding = true;
          const prev = current;
          current = flatMap2(yieldNow, () => prev);
        }
        current = this.currentTracerContext ? this.currentTracerContext(current, this) : current[evaluate](this);
        if (currentLoop !== this.currentLoopCount) {
          return Yield;
        } else if (current === Yield) {
          const yielded = this._yielded;
          if (ExitTypeId in yielded) {
            this._yielded = void 0;
            return yielded;
          }
          return Yield;
        }
      }
    } catch (error) {
      if (!hasProperty(current, evaluate)) {
        return exitDie(`Fiber.runLoop: Not a valid effect: ${String(current)}`);
      }
      return this.runLoop(exitDie(error));
    } finally {
      ;
      globalThis[currentFiberTypeId] = prevFiber;
    }
  }
  getCont(symbol4) {
    while (true) {
      const op = this._stack.pop();
      if (!op) return void 0;
      const cont = op[contAll] && op[contAll](this);
      if (cont) {
        ;
        cont[symbol4] = cont;
        return cont;
      }
      if (op[symbol4]) return op;
    }
  }
  yieldWith(value) {
    this._yielded = value;
    return Yield;
  }
  children() {
    return this._children ??= /* @__PURE__ */ new Set();
  }
  pipe() {
    return pipeArguments(this, arguments);
  }
  setContext(context3) {
    this.context = context3;
    const scheduler = this.getRef(Scheduler);
    if (scheduler !== this.currentScheduler) {
      this.currentScheduler = scheduler;
      this._dispatcher = void 0;
    }
    this.currentSpan = context3.mapUnsafe.get(ParentSpanKey);
    this.currentLogLevel = this.getRef(CurrentLogLevel);
    this.minimumLogLevel = this.getRef(MinimumLogLevel);
    this.currentStackFrame = context3.mapUnsafe.get(CurrentStackFrame.key);
    this.maxOpsBeforeYield = this.getRef(MaxOpsBeforeYield);
    this.currentPreventYield = this.getRef(PreventSchedulerYield);
    this.runtimeMetrics = context3.mapUnsafe.get(FiberRuntimeMetricsKey);
    const currentTracer = context3.mapUnsafe.get(TracerKey);
    this.currentTracerContext = currentTracer ? currentTracer["context"] : void 0;
  }
  get currentSpanLocal() {
    return this.currentSpan?._tag === "Span" ? this.currentSpan : void 0;
  }
};
var fiberMiddleware = {
  interruptChildren: void 0
};
var fiberStackAnnotations = (fiber2) => {
  if (!fiber2.currentStackFrame) return void 0;
  const annotations = /* @__PURE__ */ new Map();
  annotations.set(StackTraceKey.key, fiber2.currentStackFrame);
  return makeUnsafe(annotations);
};
var fiberAwait = (self2) => {
  const impl2 = self2;
  if (impl2._exit) return succeed3(impl2._exit);
  return callback((resume) => {
    if (impl2._exit) return resume(succeed3(impl2._exit));
    return sync(self2.addObserver((exit3) => resume(succeed3(exit3))));
  });
};
var fiberAwaitAll = (self2) => callback((resume) => {
  const iter = self2[Symbol.iterator]();
  const exits = [];
  let cancel = void 0;
  function loop() {
    let result2 = iter.next();
    while (!result2.done) {
      if (result2.value._exit) {
        exits.push(result2.value._exit);
        result2 = iter.next();
        continue;
      }
      cancel = result2.value.addObserver((exit3) => {
        exits.push(exit3);
        loop();
      });
      return;
    }
    resume(succeed3(exits));
  }
  loop();
  return sync(() => cancel?.());
});
var fiberInterrupt = (self2) => withFiber((fiber2) => fiberInterruptAs(self2, fiber2.id));
var fiberInterruptAs = /* @__PURE__ */ dual((args2) => hasProperty(args2[0], FiberTypeId), (self2, fiberId2, annotations) => withFiber((parent) => {
  let ann = fiberStackAnnotations(parent);
  ann = ann && annotations ? merge(ann, annotations) : ann ?? annotations;
  self2.interruptUnsafe(fiberId2, ann);
  return asVoid(fiberAwait(self2));
}));
var fiberInterruptAll = (fibers) => withFiber((parent) => {
  const annotations = fiberStackAnnotations(parent);
  for (const fiber2 of fibers) {
    fiber2.interruptUnsafe(parent.id, annotations);
  }
  return asVoid(fiberAwaitAll(fibers));
});
var succeed3 = exitSucceed;
var failCause = exitFailCause;
var fail3 = exitFail;
var sync = /* @__PURE__ */ makePrimitive({
  op: "Sync",
  [evaluate](fiber2) {
    const value = this[args]();
    const cont = fiber2.getCont(contA);
    return cont ? cont[contA](value, fiber2) : fiber2.yieldWith(exitSucceed(value));
  }
});
var suspend = /* @__PURE__ */ makePrimitive({
  op: "Suspend",
  [evaluate](_fiber) {
    return this[args]();
  }
});
var fromResult = /* @__PURE__ */ match3({
  onFailure: fail3,
  onSuccess: succeed3
});
var yieldNowWith = /* @__PURE__ */ makePrimitive({
  op: "Yield",
  [evaluate](fiber2) {
    let resumed = false;
    fiber2.currentDispatcher.scheduleTask(() => {
      if (resumed) return;
      fiber2.evaluate(exitVoid);
    }, this[args] ?? 0);
    return fiber2.yieldWith(() => {
      resumed = true;
    });
  }
});
var yieldNow = /* @__PURE__ */ yieldNowWith(0);
var succeedSome = (a) => succeed3(some2(a));
var succeedNone = /* @__PURE__ */ succeed3(/* @__PURE__ */ none2());
var failCauseSync = (evaluate2) => suspend(() => failCause(internalCall(evaluate2)));
var die = (defect) => exitDie(defect);
var failSync = (error) => suspend(() => fail3(internalCall(error)));
var void_ = /* @__PURE__ */ succeed3(void 0);
var try_ = (options) => {
  const evaluate2 = typeof options === "function" ? options : options.try;
  const catcher = typeof options === "function" ? (cause) => new UnknownError(cause, "An error occurred in Effect.try") : options.catch;
  return suspend(() => {
    try {
      return succeed3(internalCall(evaluate2));
    } catch (err) {
      return fail3(internalCall(() => catcher(err)));
    }
  });
};
var tryPromise = (options) => {
  const f = typeof options === "function" ? options : options.try;
  const catcher = typeof options === "function" ? (cause) => new UnknownError(cause, "An error occurred in Effect.tryPromise") : options.catch;
  return callbackOptions(function(resume, signal) {
    const failWithCatch = (cause) => {
      try {
        resume(fail3(internalCall(() => catcher(cause))));
      } catch (err) {
        resume(die(err));
      }
    };
    try {
      internalCall(() => f(signal)).then((a) => resume(succeed3(a)), failWithCatch);
    } catch (err) {
      failWithCatch(err);
    }
  }, f.length !== 0);
};
var callbackOptions = /* @__PURE__ */ makePrimitive({
  op: "Async",
  single: false,
  [evaluate](fiber2) {
    const register = internalCall(() => this[args][0].bind(fiber2.currentScheduler));
    let resumed = false;
    let yielded = false;
    const controller = this[args][1] ? new AbortController() : void 0;
    const onCancel = register((effect2) => {
      if (resumed) return;
      resumed = true;
      if (yielded) {
        fiber2.evaluate(effect2);
      } else {
        yielded = effect2;
      }
    }, controller?.signal);
    if (yielded !== false) return yielded;
    yielded = true;
    fiber2._yielded = () => {
      resumed = true;
    };
    if (controller === void 0 && onCancel === void 0) {
      return Yield;
    }
    fiber2._stack.push(asyncFinalizer(() => {
      resumed = true;
      controller?.abort();
      return onCancel ?? exitVoid;
    }));
    return Yield;
  }
});
var asyncFinalizer = /* @__PURE__ */ makePrimitive({
  op: "AsyncFinalizer",
  [contAll](fiber2) {
    if (fiber2.interruptible) {
      fiber2.interruptible = false;
      fiber2._stack.push(setInterruptibleTrue);
    }
  },
  [contE](cause, _fiber) {
    return hasInterrupts(cause) ? flatMap2(this[args](), () => failCause(cause)) : failCause(cause);
  }
});
var callback = (register) => callbackOptions(register, register.length >= 2);
var gen = (...args2) => suspend(() => fromIteratorUnsafe(args2.length === 1 ? args2[0]() : args2[1].call(args2[0].self)));
var fnUntraced = (body, ...pipeables) => {
  const fn2 = pipeables.length === 0 ? function() {
    return suspend(() => fromIteratorUnsafe(body.apply(this, arguments)));
  } : function() {
    let effect2 = suspend(() => fromIteratorUnsafe(body.apply(this, arguments)));
    for (let i = 0; i < pipeables.length; i++) {
      effect2 = pipeables[i](effect2, ...arguments);
    }
    return effect2;
  };
  return defineFunctionLength(body.length, fn2);
};
var defineFunctionLength = (length, fn2) => Object.defineProperty(fn2, "length", {
  value: length,
  configurable: true
});
var fnUntracedEager = (body, ...pipeables) => defineFunctionLength(body.length, pipeables.length === 0 ? function() {
  return fromIteratorEagerUnsafe(() => body.apply(this, arguments));
} : function() {
  let effect2 = fromIteratorEagerUnsafe(() => body.apply(this, arguments));
  for (const pipeable of pipeables) {
    effect2 = pipeable(effect2);
  }
  return effect2;
});
var fromIteratorEagerUnsafe = (evaluate2) => {
  try {
    const iterator = evaluate2();
    let value = void 0;
    while (true) {
      const state = iterator.next(value);
      if (state.done) {
        return succeed3(state.value);
      }
      const primitive = state.value;
      if (primitive && primitive._tag === "Success") {
        value = primitive.value;
        continue;
      } else if (primitive && primitive._tag === "Failure") {
        return state.value;
      } else {
        let isFirstExecution = true;
        return suspend(() => {
          if (isFirstExecution) {
            isFirstExecution = false;
            return flatMap2(state.value, (value2) => fromIteratorUnsafe(iterator, value2));
          } else {
            return suspend(() => fromIteratorUnsafe(evaluate2()));
          }
        });
      }
    }
  } catch (error) {
    return die(error);
  }
};
var fromIteratorUnsafe = /* @__PURE__ */ makePrimitive({
  op: "Iterator",
  single: false,
  [contA](value, fiber2) {
    const iter = this[args][0];
    while (true) {
      const state = iter.next(value);
      if (state.done) return succeed3(state.value);
      if (!effectIsExit(state.value)) {
        fiber2._stack.push(this);
        return state.value;
      } else if (state.value._tag === "Failure") {
        return state.value;
      }
      value = state.value.value;
    }
  },
  [evaluate](fiber2) {
    return this[contA](this[args][1], fiber2);
  }
});
var as = /* @__PURE__ */ dual(2, (self2, value) => {
  const b = succeed3(value);
  return flatMap2(self2, (_) => b);
});
var asSome = (self2) => map3(self2, some2);
var andThen = /* @__PURE__ */ dual(2, (self2, f) => flatMap2(self2, (a) => isEffect(f) ? f : internalCall(() => f(a))));
var tap = /* @__PURE__ */ dual(2, (self2, f) => flatMap2(self2, (a) => as(isEffect(f) ? f : internalCall(() => f(a)), a)));
var asVoid = (self2) => flatMap2(self2, (_) => exitVoid);
var flatMap2 = /* @__PURE__ */ dual(2, (self2, f) => {
  const onSuccess = Object.create(OnSuccessProto);
  onSuccess[args] = self2;
  onSuccess[contA] = f.length !== 1 ? (a) => f(a) : f;
  return onSuccess;
});
var OnSuccessProto = /* @__PURE__ */ makePrimitiveProto({
  op: "OnSuccess",
  [evaluate](fiber2) {
    fiber2._stack.push(this);
    return this[args];
  }
});
var effectIsExit = (effect2) => ExitTypeId in effect2;
var flatMapEager = /* @__PURE__ */ dual(2, (self2, f) => {
  if (effectIsExit(self2)) {
    return self2._tag === "Success" ? f(self2.value) : self2;
  }
  return flatMap2(self2, f);
});
var flatten = (self2) => flatMap2(self2, identity);
var map3 = /* @__PURE__ */ dual(2, (self2, f) => flatMap2(self2, (a) => succeed3(internalCall(() => f(a)))));
var mapEager = /* @__PURE__ */ dual(2, (self2, f) => effectIsExit(self2) ? exitMap(self2, f) : map3(self2, f));
var mapErrorEager = /* @__PURE__ */ dual(2, (self2, f) => effectIsExit(self2) ? exitMapError(self2, f) : mapError2(self2, f));
var exitIsSuccess = (self2) => self2._tag === "Success";
var exitVoid = /* @__PURE__ */ exitSucceed(void 0);
var exitMap = /* @__PURE__ */ dual(2, (self2, f) => self2._tag === "Success" ? exitSucceed(f(self2.value)) : self2);
var exitMapError = /* @__PURE__ */ dual(2, (self2, f) => {
  if (self2._tag === "Success") return self2;
  const error = findError(self2.cause);
  if (isFailure2(error)) return self2;
  return exitFail(f(error.success));
});
var exitAsVoidAll = (exits) => {
  const failures = [];
  for (const exit3 of exits) {
    if (exit3._tag === "Failure") {
      failures.push(...exit3.cause.reasons);
    }
  }
  return failures.length === 0 ? exitVoid : exitFailCause(causeFromReasons(failures));
};
var serviceOption = (service3) => withFiber((fiber2) => succeed3(getOption(fiber2.context, service3)));
var updateContext = /* @__PURE__ */ dual(2, (self2, f) => withFiber((fiber2) => {
  const prevContext = fiber2.context;
  const nextContext = f(prevContext);
  if (prevContext === nextContext) return self2;
  fiber2.setContext(nextContext);
  return onExitPrimitive(self2, () => {
    fiber2.setContext(prevContext);
    return void 0;
  });
}));
var updateService = /* @__PURE__ */ dual(3, (self2, service3, f) => updateContext(self2, (s) => {
  const prev = getUnsafe(s, service3);
  const next = f(prev);
  if (prev === next) return s;
  return add(s, service3, next);
}));
var contextWith = (f) => withFiber((fiber2) => f(fiber2.context));
var provideContext = /* @__PURE__ */ dual(2, (self2, context3) => {
  if (effectIsExit(self2)) return self2;
  return updateContext(self2, merge(context3));
});
var provideService = function() {
  if (arguments.length === 1) {
    return dual(2, (self2, impl2) => provideServiceImpl(self2, arguments[0], impl2));
  }
  return dual(3, (self2, service3, impl2) => provideServiceImpl(self2, service3, impl2)).apply(this, arguments);
};
var provideServiceImpl = (self2, service3, implementation) => updateContext(self2, (s) => {
  const prev = s.mapUnsafe.get(service3.key);
  if (prev === implementation) return s;
  return add(s, service3, implementation);
});
var filterOrFail = /* @__PURE__ */ dual((args2) => isEffect(args2[0]), (self2, predicate, orFailWith) => filterOrElse(self2, predicate, orFailWith ? (a) => fail3(orFailWith(a)) : () => fail3(new NoSuchElementError())));
var forever = /* @__PURE__ */ dual((args2) => isEffect(args2[0]), (self2, options) => whileLoop({
  while: constTrue,
  body: constant(options?.disableYield ? self2 : flatMap2(self2, (_) => yieldNow)),
  step: constVoid
}));
var catchCause = /* @__PURE__ */ dual(2, (self2, f) => {
  const onFailure = Object.create(OnFailureProto);
  onFailure[args] = self2;
  onFailure[contE] = f.length !== 1 ? (cause) => f(cause) : f;
  return onFailure;
});
var OnFailureProto = /* @__PURE__ */ makePrimitiveProto({
  op: "OnFailure",
  [evaluate](fiber2) {
    fiber2._stack.push(this);
    return this[args];
  }
});
var catchCauseFilter = /* @__PURE__ */ dual(3, (self2, filter5, f) => catchCause(self2, (cause) => {
  const eb2 = filter5(cause);
  return isFailure2(eb2) ? failCause(eb2.failure) : internalCall(() => f(eb2.success, cause));
}));
var catch_ = /* @__PURE__ */ dual(2, (self2, f) => catchCauseFilter(self2, findError, (e) => f(e)));
var catchDefect = /* @__PURE__ */ dual(2, (self2, f) => catchCauseFilter(self2, findDefect, f));
var catchIf = /* @__PURE__ */ dual((args2) => isEffect(args2[0]), (self2, predicate, f, orElse) => catchCause(self2, (cause) => {
  const error = findError(cause);
  if (isFailure2(error)) return failCause(error.failure);
  if (!predicate(error.success)) {
    return orElse ? internalCall(() => orElse(error.success)) : failCause(cause);
  }
  return internalCall(() => f(error.success));
}));
var catchTag = /* @__PURE__ */ dual((args2) => isEffect(args2[0]), (self2, k, f, orElse) => {
  const pred = Array.isArray(k) ? (e) => hasProperty(e, "_tag") && k.includes(e._tag) : isTagged(k);
  return catchIf(self2, pred, f, orElse);
});
var mapError2 = /* @__PURE__ */ dual(2, (self2, f) => catch_(self2, (error) => failSync(() => f(error))));
var orDie = (self2) => catch_(self2, die);
var result = (self2) => matchEager(self2, {
  onFailure: fail2,
  onSuccess: succeed2
});
var matchCauseEffect = /* @__PURE__ */ dual(2, (self2, options) => {
  const primitive = Object.create(OnSuccessAndFailureProto);
  primitive[args] = self2;
  primitive[contA] = options.onSuccess.length !== 1 ? (a) => options.onSuccess(a) : options.onSuccess;
  primitive[contE] = options.onFailure.length !== 1 ? (cause) => options.onFailure(cause) : options.onFailure;
  return primitive;
});
var OnSuccessAndFailureProto = /* @__PURE__ */ makePrimitiveProto({
  op: "OnSuccessAndFailure",
  [evaluate](fiber2) {
    fiber2._stack.push(this);
    return this[args];
  }
});
var matchEffect = /* @__PURE__ */ dual(2, (self2, options) => matchCauseEffect(self2, {
  onFailure: (cause) => {
    const fail9 = cause.reasons.find(isFailReason);
    return fail9 ? internalCall(() => options.onFailure(fail9.error)) : failCause(cause);
  },
  onSuccess: options.onSuccess
}));
var match4 = /* @__PURE__ */ dual(2, (self2, options) => matchEffect(self2, {
  onFailure: (error) => sync(() => options.onFailure(error)),
  onSuccess: (value) => sync(() => options.onSuccess(value))
}));
var matchEager = /* @__PURE__ */ dual(2, (self2, options) => {
  if (effectIsExit(self2)) {
    if (self2._tag === "Success") return exitSucceed(options.onSuccess(self2.value));
    const error = findError(self2.cause);
    if (isFailure2(error)) return self2;
    return exitSucceed(options.onFailure(error.success));
  }
  return match4(self2, options);
});
var exit = (self2) => effectIsExit(self2) ? exitSucceed(self2) : exitPrimitive(self2);
var exitPrimitive = /* @__PURE__ */ makePrimitive({
  op: "Exit",
  [evaluate](fiber2) {
    fiber2._stack.push(this);
    return this[args];
  },
  [contA](value, _, exit3) {
    return succeed3(exit3 ?? exitSucceed(value));
  },
  [contE](cause, _, exit3) {
    return succeed3(exit3 ?? exitFailCause(cause));
  }
});
var ScopeTypeId = "~effect/Scope";
var ScopeCloseableTypeId = "~effect/Scope/Closeable";
var scopeTag = /* @__PURE__ */ Service("effect/Scope");
var scopeClose = (self2, exit_) => suspend(() => scopeCloseUnsafe(self2, exit_) ?? void_);
var scopeCloseUnsafe = (self2, exit_) => {
  if (self2.state._tag === "Closed") return;
  const closed = {
    _tag: "Closed",
    exit: exit_
  };
  if (self2.state._tag === "Empty") {
    self2.state = closed;
    return;
  }
  const {
    finalizers
  } = self2.state;
  self2.state = closed;
  if (finalizers.size === 0) {
    return;
  } else if (finalizers.size === 1) {
    return finalizers.values().next().value(exit_);
  }
  return scopeCloseFinalizers(self2, finalizers, exit_);
};
var scopeCloseFinalizers = /* @__PURE__ */ fnUntraced(function* (self2, finalizers, exit_) {
  let exits = [];
  const fibers = [];
  const arr = Array.from(finalizers.values());
  const parent = getCurrentFiber();
  for (let i = arr.length - 1; i >= 0; i--) {
    const finalizer = arr[i];
    if (self2.strategy === "sequential") {
      exits.push(yield* exit(finalizer(exit_)));
    } else {
      fibers.push(forkUnsafe(parent, finalizer(exit_), true, true, "inherit"));
    }
  }
  if (fibers.length > 0) {
    exits = yield* fiberAwaitAll(fibers);
  }
  return yield* exitAsVoidAll(exits);
});
var scopeForkUnsafe = (scope3, finalizerStrategy) => {
  const newScope = scopeMakeUnsafe(finalizerStrategy);
  if (scope3.state._tag === "Closed") {
    newScope.state = scope3.state;
    return newScope;
  }
  const key = {};
  scopeAddFinalizerUnsafe(scope3, key, (exit3) => scopeClose(newScope, exit3));
  scopeAddFinalizerUnsafe(newScope, key, (_) => sync(() => scopeRemoveFinalizerUnsafe(scope3, key)));
  return newScope;
};
var scopeAddFinalizerExit = (scope3, finalizer) => {
  return suspend(() => {
    if (scope3.state._tag === "Closed") {
      return finalizer(scope3.state.exit);
    }
    scopeAddFinalizerUnsafe(scope3, {}, finalizer);
    return void_;
  });
};
var scopeAddFinalizer = (scope3, finalizer) => scopeAddFinalizerExit(scope3, constant(finalizer));
var scopeAddFinalizerUnsafe = (scope3, key, finalizer) => {
  if (scope3.state._tag === "Empty") {
    scope3.state = {
      _tag: "Open",
      finalizers: /* @__PURE__ */ new Map([[key, finalizer]])
    };
  } else if (scope3.state._tag === "Open") {
    scope3.state.finalizers.set(key, finalizer);
  }
};
var scopeRemoveFinalizerUnsafe = (scope3, key) => {
  if (scope3.state._tag === "Open") {
    scope3.state.finalizers.delete(key);
  }
};
var scopeMakeUnsafe = (finalizerStrategy = "sequential") => ({
  [ScopeCloseableTypeId]: ScopeCloseableTypeId,
  [ScopeTypeId]: ScopeTypeId,
  strategy: finalizerStrategy,
  state: constScopeEmpty
});
var constScopeEmpty = {
  _tag: "Empty"
};
var scopeMake = (finalizerStrategy) => sync(() => scopeMakeUnsafe(finalizerStrategy));
var scope = scopeTag;
var provideScope = /* @__PURE__ */ provideService(scopeTag);
var scoped = (self2) => withFiber((fiber2) => {
  const prev = fiber2.context;
  const scope3 = scopeMakeUnsafe();
  fiber2.setContext(add(fiber2.context, scopeTag, scope3));
  return onExitPrimitive(self2, (exit3) => {
    fiber2.setContext(prev);
    return scopeCloseUnsafe(scope3, exit3);
  });
});
var acquireRelease = (acquire, release, options) => contextWith((context3) => uninterruptibleMask((restore) => flatMap2(scope, (scope3) => tap(options?.interruptible ? restore(acquire) : acquire, (a) => scopeAddFinalizerExit(scope3, (exit3) => provideContext(release(a, exit3), context3))))));
var addFinalizer = (finalizer) => flatMap2(scope, (scope3) => contextWith((context3) => scopeAddFinalizerExit(scope3, (exit3) => provideContext(finalizer(exit3), context3))));
var onExitPrimitive = /* @__PURE__ */ makePrimitive({
  op: "OnExit",
  single: false,
  [evaluate](fiber2) {
    fiber2._stack.push(this);
    return this[args][0];
  },
  [contAll](fiber2) {
    if (fiber2.interruptible && this[args][2] !== true) {
      fiber2._stack.push(setInterruptibleTrue);
      fiber2.interruptible = false;
    }
  },
  [contA](value, _, exit3) {
    exit3 ??= exitSucceed(value);
    const eff = this[args][1](exit3);
    return eff ? flatMap2(eff, (_2) => exit3) : exit3;
  },
  [contE](cause, _, exit3) {
    exit3 ??= exitFailCause(cause);
    const eff = this[args][1](exit3);
    return eff ? flatMap2(eff, (_2) => exit3) : exit3;
  }
});
var onExit = /* @__PURE__ */ dual(2, onExitPrimitive);
var ensuring = /* @__PURE__ */ dual(2, (self2, finalizer) => onExit(self2, (_) => finalizer));
var uninterruptible = (self2) => withFiber((fiber2) => {
  if (!fiber2.interruptible) return self2;
  fiber2.interruptible = false;
  fiber2._stack.push(setInterruptibleTrue);
  return self2;
});
var setInterruptible = /* @__PURE__ */ makePrimitive({
  op: "SetInterruptible",
  [contAll](fiber2) {
    fiber2.interruptible = this[args];
    if (fiber2._interruptedCause && fiber2.interruptible) {
      return () => failCause(fiber2._interruptedCause);
    }
  }
});
var setInterruptibleTrue = /* @__PURE__ */ setInterruptible(true);
var setInterruptibleFalse = /* @__PURE__ */ setInterruptible(false);
var interruptible = (self2) => withFiber((fiber2) => {
  if (fiber2.interruptible) return self2;
  fiber2.interruptible = true;
  fiber2._stack.push(setInterruptibleFalse);
  if (fiber2._interruptedCause) return failCause(fiber2._interruptedCause);
  return self2;
});
var uninterruptibleMask = (f) => withFiber((fiber2) => {
  if (!fiber2.interruptible) return f(identity);
  fiber2.interruptible = false;
  fiber2._stack.push(setInterruptibleTrue);
  return f(interruptible);
});
var all = (arg, options) => {
  if (isIterable(arg)) {
    return options?.mode === "result" ? forEach(arg, result, options) : forEach(arg, identity, options);
  } else if (options?.discard) {
    return options.mode === "result" ? forEach(Object.values(arg), result, options) : forEach(Object.values(arg), identity, options);
  }
  return suspend(() => {
    const out = {};
    return as(forEach(Object.entries(arg), ([key, effect2]) => map3(options?.mode === "result" ? result(effect2) : effect2, (value) => {
      out[key] = value;
    }), {
      discard: true,
      concurrency: options?.concurrency
    }), out);
  });
};
var whileLoop = /* @__PURE__ */ makePrimitive({
  op: "While",
  [contA](value, fiber2) {
    this[args].step(value);
    if (this[args].while()) {
      fiber2._stack.push(this);
      return this[args].body();
    }
    return exitVoid;
  },
  [evaluate](fiber2) {
    if (this[args].while()) {
      fiber2._stack.push(this);
      return this[args].body();
    }
    return exitVoid;
  }
});
var forEach = /* @__PURE__ */ dual((args2) => typeof args2[1] === "function", (iterable, f, options) => withFiber((parent) => {
  const concurrencyOption = options?.concurrency === "inherit" ? parent.getRef(CurrentConcurrency) : options?.concurrency ?? 1;
  const concurrency = concurrencyOption === "unbounded" ? Number.POSITIVE_INFINITY : Math.max(1, concurrencyOption);
  if (concurrency === 1) {
    return forEachSequential(iterable, f, options);
  }
  const items = fromIterable(iterable);
  let length = items.length;
  if (length === 0) {
    return options?.discard ? void_ : succeed3([]);
  }
  const out = options?.discard ? void 0 : new Array(length);
  const eff = forEachConcurrent({
    f,
    out
  }, items, {
    concurrency
  });
  return eff ? as(eff, out) : succeed3(out);
}));
var forEachSequential = (iterable, f, options) => suspend(() => {
  const out = options?.discard ? void 0 : [];
  const iterator = iterable[Symbol.iterator]();
  let state = iterator.next();
  let index = 0;
  return as(whileLoop({
    while: () => !state.done,
    body: () => f(state.value, index++),
    step: (b) => {
      if (out) out.push(b);
      state = iterator.next();
    }
  }), out);
});
var iterateEagerImpl = (options) => {
  const onItem = options.onItem;
  const step = options.step;
  return (state, items, opts) => {
    let index = opts?.start ?? 0;
    const end = opts?.end ?? items.length;
    const concurrency = opts?.concurrency ?? 1;
    let done4 = false;
    let parentFiber;
    let fibers;
    let resume;
    let interrupted = false;
    let terminal;
    let effect2;
    const go = () => {
      let paused = false;
      for (; !terminal && index < end; index++) {
        const item = items[index];
        const eff = effect2 ?? onItem(state, item, index);
        if (effectIsExit(eff)) {
          terminal = step(state, item, eff, index);
          if (terminal) break;
        } else if (concurrency === 1) {
          return flatMap2(exit(eff), (exit3) => {
            terminal = step(state, item, exit3, index);
            index++;
            return terminal ?? go() ?? void_;
          });
        } else if (!parentFiber) {
          return callback((cb2) => {
            parentFiber = getCurrentFiber();
            effect2 = eff;
            resume = cb2;
            const result2 = go();
            if (result2) return cb2(result2);
            return suspend(() => {
              terminal = exitVoid;
              interrupted = true;
              return fibers ? fiberInterruptAll(fibers) : void_;
            });
          });
        } else {
          effect2 = void 0;
          const fiber2 = forkUnsafe(parentFiber, eff, true, true, "inherit");
          if (fiber2._exit) {
            terminal = step(state, item, fiber2._exit, index);
            if (terminal) break;
            continue;
          }
          if (fibers) fibers.add(fiber2);
          else fibers = /* @__PURE__ */ new Set([fiber2]);
          const currentIndex = index;
          fiber2.addObserver((exit3) => {
            fibers.delete(fiber2);
            if (terminal) {
              if (!interrupted && exit3._tag === "Failure") {
                for (const reason of exit3.cause.reasons) {
                  if (reason._tag === "Interrupt") continue;
                  else if (terminal._tag === "Failure") {
                    ;
                    terminal.cause.reasons.push(reason);
                  } else {
                    terminal = exitFailCause(causeFromReasons([reason]));
                  }
                }
              }
            } else {
              const result2 = step(state, item, exit3, currentIndex);
              if (result2) {
                terminal = result2._tag === "Failure" ? exitFailCause(causeFromReasons(result2.cause.reasons.slice())) : result2;
                go();
              }
            }
            if (paused) {
              const eff2 = go();
              if (eff2) resume(eff2);
            } else if (done4 && fibers.size === 0) {
              resume(terminal ?? void_);
            }
          });
          if (fibers.size < concurrency) continue;
          paused = true;
          index++;
          return;
        }
      }
      done4 = true;
      if (terminal) {
        if (fibers && fibers.size > 0) {
          const annotations = fiberStackAnnotations(parentFiber);
          fibers.forEach((f) => f.interruptUnsafe(parentFiber.id, annotations));
          return;
        }
        if (resume || terminal._tag === "Failure") {
          return terminal;
        }
      } else if (resume) {
        if (!fibers) {
          return exitVoid;
        } else if (fibers.size === 0) {
          resume(void_);
        }
      }
    };
    return go();
  };
};
var iterateEager = () => iterateEagerImpl;
var forEachConcurrent = /* @__PURE__ */ iterateEagerImpl({
  onItem(state, item, index) {
    return state.f(item, index);
  },
  step(state, _, exit3, index) {
    if (exit3._tag === "Failure") return exit3;
    else if (state.out) {
      state.out[index] = exit3.value;
    }
  }
});
var filterOrElse = /* @__PURE__ */ dual(3, (self2, predicate, orElse) => flatMap2(self2, (a) => predicate(a) ? succeed3(a) : orElse(a)));
var forkUnsafe = (parent, effect2, immediate = false, daemon = false, uninterruptible3 = false) => {
  const interruptible2 = uninterruptible3 === "inherit" ? parent.interruptible : !uninterruptible3;
  const child = new FiberImpl(parent.context, interruptible2);
  if (immediate) {
    child.evaluate(effect2);
  } else {
    parent.currentDispatcher.scheduleTask(() => child.evaluate(effect2), 0);
  }
  if (!daemon && !child._exit) {
    parent.children().add(child);
    child.addObserver(() => parent._children.delete(child));
  }
  return child;
};
var runForkWith = (context3) => (effect2, options) => {
  const fiber2 = new FiberImpl(options?.scheduler ? add(context3, Scheduler, options.scheduler) : context3, options?.uninterruptible !== true);
  fiber2.evaluate(effect2);
  if (fiber2._exit) return fiber2;
  if (options?.signal) {
    if (options.signal.aborted) {
      fiber2.interruptUnsafe();
    } else {
      const abort = () => fiber2.interruptUnsafe();
      options.signal.addEventListener("abort", abort, {
        once: true
      });
      fiber2.addObserver(() => options.signal.removeEventListener("abort", abort));
    }
  }
  if (options?.onFiberStart) {
    options.onFiberStart(fiber2);
  }
  return fiber2;
};
var fiberRunIn = /* @__PURE__ */ dual(2, (self2, scope3) => {
  if (self2._exit) {
    return self2;
  } else if (scope3.state._tag === "Closed") {
    self2.interruptUnsafe(self2.id);
    return self2;
  }
  const key = {};
  scopeAddFinalizerUnsafe(scope3, key, () => fiberInterrupt(self2));
  self2.addObserver(() => scopeRemoveFinalizerUnsafe(scope3, key));
  return self2;
});
var runFork = /* @__PURE__ */ runForkWith(/* @__PURE__ */ empty());
var runCallbackWith = (context3) => {
  const runFork3 = runForkWith(context3);
  return (effect2, options) => {
    const fiber2 = runFork3(effect2, options);
    if (options?.onExit) {
      fiber2.addObserver(options.onExit);
    }
    return (interruptor) => {
      return fiber2.interruptUnsafe(interruptor);
    };
  };
};
var runCallback = /* @__PURE__ */ runCallbackWith(/* @__PURE__ */ empty());
var runPromiseExitWith = (context3) => {
  const runFork3 = runForkWith(context3);
  return (effect2, options) => {
    const fiber2 = runFork3(effect2, options);
    return new Promise((resolve2) => {
      fiber2.addObserver((exit3) => resolve2(exit3));
    });
  };
};
var runPromiseExit = /* @__PURE__ */ runPromiseExitWith(/* @__PURE__ */ empty());
var runPromiseWith = (context3) => {
  const runPromiseExit3 = runPromiseExitWith(context3);
  return (effect2, options) => runPromiseExit3(effect2, options).then((exit3) => {
    if (exit3._tag === "Failure") {
      throw causeSquash(exit3.cause);
    }
    return exit3.value;
  });
};
var runPromise = /* @__PURE__ */ runPromiseWith(/* @__PURE__ */ empty());
var runSyncExitWith = (context3) => {
  const runFork3 = runForkWith(context3);
  return (effect2) => {
    if (effectIsExit(effect2)) return effect2;
    const scheduler = new MixedScheduler("sync");
    const fiber2 = runFork3(effect2, {
      scheduler
    });
    fiber2.currentDispatcher?.flush();
    return fiber2._exit ?? exitDie(new AsyncFiberError(fiber2));
  };
};
var runSyncExit = /* @__PURE__ */ runSyncExitWith(/* @__PURE__ */ empty());
var runSyncWith = (context3) => {
  const runSyncExit3 = runSyncExitWith(context3);
  return (effect2) => {
    const exit3 = runSyncExit3(effect2);
    if (exit3._tag === "Failure") throw causeSquash(exit3.cause);
    return exit3.value;
  };
};
var runSync = /* @__PURE__ */ runSyncWith(/* @__PURE__ */ empty());
var bigint02 = /* @__PURE__ */ BigInt(0);
var NoopSpanProto = {
  _tag: "Span",
  spanId: "noop",
  traceId: "noop",
  sampled: false,
  status: {
    _tag: "Ended",
    startTime: bigint02,
    endTime: bigint02,
    exit: exitVoid
  },
  attributes: /* @__PURE__ */ new Map(),
  links: [],
  kind: "internal",
  attribute() {
  },
  event() {
  },
  end() {
  },
  addLinks() {
  }
};
var noopSpan = (options) => Object.assign(Object.create(NoopSpanProto), options);
var filterDisablePropagation = (span) => {
  if (!span) return none2();
  return get(span.annotations, DisablePropagation) ? span._tag === "Span" ? filterDisablePropagation(getOrUndefined(span.parent)) : none2() : some2(span);
};
var makeSpanUnsafe = (fiber2, name, options) => {
  const disablePropagation = !fiber2.getRef(TracerEnabled) || options?.annotations && get(options.annotations, DisablePropagation);
  const parent = options?.parent !== void 0 ? some2(options.parent) : options?.root ? none2() : filterDisablePropagation(fiber2.currentSpan);
  let span;
  if (disablePropagation) {
    span = noopSpan({
      name,
      parent,
      annotations: add(options?.annotations ?? empty(), DisablePropagation, true)
    });
  } else {
    const tracer2 = fiber2.getRef(Tracer);
    const clock = fiber2.getRef(ClockRef);
    const timingEnabled = fiber2.getRef(TracerTimingEnabled);
    const annotationsFromEnv = fiber2.getRef(TracerSpanAnnotations);
    const linksFromEnv = fiber2.getRef(TracerSpanLinks);
    const level = options?.level ?? fiber2.getRef(CurrentTraceLevel);
    const links = options?.links !== void 0 ? [...linksFromEnv, ...options.links] : linksFromEnv.slice();
    span = tracer2.span({
      name,
      parent,
      annotations: options?.annotations ?? empty(),
      links,
      startTime: timingEnabled ? clock.currentTimeNanosUnsafe() : BigInt(0),
      kind: options?.kind ?? "internal",
      root: options?.root ?? isNone2(parent),
      sampled: options?.sampled ?? (isSome2(parent) && parent.value.sampled === false ? false : !isLogLevelGreaterThan(fiber2.getRef(MinimumTraceLevel), level))
    });
    for (const [key, value] of Object.entries(annotationsFromEnv)) {
      span.attribute(key, value);
    }
    if (options?.attributes !== void 0) {
      for (const [key, value] of Object.entries(options.attributes)) {
        span.attribute(key, value);
      }
    }
  }
  return span;
};
var makeSpanScoped = (name, options) => uninterruptible(withFiber((fiber2) => {
  const scope3 = getUnsafe(fiber2.context, scopeTag);
  const span = makeSpanUnsafe(fiber2, name, options ?? {});
  const clock = fiber2.getRef(ClockRef);
  const timingEnabled = fiber2.getRef(TracerTimingEnabled);
  return as(scopeAddFinalizerExit(scope3, (exit3) => endSpan(span, exit3, clock, timingEnabled)), span);
}));
var provideSpanStackFrame = (name, stack) => {
  stack = typeof stack === "function" ? stack : constUndefined;
  return updateService(CurrentStackFrame, (parent) => ({
    name,
    stack,
    parent
  }));
};
var endSpan = (span, exit3, clock, timingEnabled) => sync(() => {
  if (span.status._tag === "Ended") return;
  span.end(timingEnabled ? clock.currentTimeNanosUnsafe() : bigint02, exit3);
});
var useSpan = (name, ...args2) => {
  const options = args2.length === 1 ? void 0 : args2[0];
  const evaluate2 = args2[args2.length - 1];
  return withFiber((fiber2) => {
    const span = makeSpanUnsafe(fiber2, name, options);
    const clock = fiber2.getRef(ClockRef);
    return onExit(internalCall(() => evaluate2(span)), (exit3) => sync(() => {
      if (span.status._tag === "Ended") return;
      span.end(clock.currentTimeNanosUnsafe(), exit3);
    }));
  });
};
var provideParentSpan = /* @__PURE__ */ provideService(ParentSpan);
var withParentSpan = function() {
  const dataFirst = isEffect(arguments[0]);
  const span = dataFirst ? arguments[1] : arguments[0];
  let options = dataFirst ? arguments[2] : arguments[1];
  let provideStackFrame = identity;
  if (span._tag === "Span") {
    options = addSpanStackTrace(options);
    provideStackFrame = provideSpanStackFrame(span.name, options?.captureStackTrace);
  }
  if (dataFirst) {
    return provideParentSpan(provideStackFrame(arguments[0]), span);
  }
  return (self2) => provideParentSpan(provideStackFrame(self2), span);
};
var withSpan = function() {
  const dataFirst = typeof arguments[0] !== "string";
  const name = dataFirst ? arguments[1] : arguments[0];
  const traceOptions = addSpanStackTrace(arguments[2]);
  if (dataFirst) {
    const self2 = arguments[0];
    return useSpan(name, arguments[2], (span) => withParentSpan(self2, span, traceOptions));
  }
  const fnArg = typeof arguments[1] === "function" ? arguments[1] : void 0;
  const options = fnArg ? void 0 : arguments[1];
  return (self2, ...args2) => useSpan(name, fnArg ? fnArg(...args2) : options, (span) => withParentSpan(self2, span, traceOptions));
};
var ClockRef = /* @__PURE__ */ Reference("effect/Clock", {
  defaultValue: () => new ClockImpl()
});
var MAX_TIMER_MILLIS = 2 ** 31 - 1;
var ClockImpl = class {
  currentTimeMillisUnsafe() {
    return Date.now();
  }
  currentTimeMillis = /* @__PURE__ */ sync(() => this.currentTimeMillisUnsafe());
  currentTimeNanosUnsafe() {
    return processOrPerformanceNow();
  }
  currentTimeNanos = /* @__PURE__ */ sync(() => this.currentTimeNanosUnsafe());
  sleep(duration) {
    const millis2 = toMillis(duration);
    if (millis2 <= 0) return yieldNow;
    return callback((resume) => {
      if (millis2 > MAX_TIMER_MILLIS) return;
      const handle = setTimeout(() => resume(void_), millis2);
      return sync(() => clearTimeout(handle));
    });
  }
};
var performanceNowNanos = /* @__PURE__ */ (function() {
  const bigint1e6 = /* @__PURE__ */ BigInt(1e6);
  if (typeof performance === "undefined" || typeof performance.now === "undefined") {
    return () => BigInt(Date.now()) * bigint1e6;
  }
  let origin;
  return () => {
    origin ??= BigInt(Date.now()) * bigint1e6 - BigInt(Math.round(performance.now() * 1e6));
    return origin + BigInt(Math.round(performance.now() * 1e6));
  };
})();
var processOrPerformanceNow = /* @__PURE__ */ (function() {
  const processHrtime = typeof process === "object" && "hrtime" in process && typeof process.hrtime.bigint === "function" ? process.hrtime : void 0;
  if (!processHrtime) {
    return performanceNowNanos;
  }
  const origin = /* @__PURE__ */ BigInt(/* @__PURE__ */ Date.now()) * /* @__PURE__ */ BigInt(1e6) - /* @__PURE__ */ processHrtime.bigint();
  return () => origin + processHrtime.bigint();
})();
var TimeoutErrorTypeId = "~effect/Cause/TimeoutError";
var TimeoutError = class extends (/* @__PURE__ */ TaggedError("TimeoutError")) {
  [TimeoutErrorTypeId] = TimeoutErrorTypeId;
  constructor(message) {
    super({
      message
    });
  }
};
var IllegalArgumentErrorTypeId = "~effect/Cause/IllegalArgumentError";
var IllegalArgumentError = class extends (/* @__PURE__ */ TaggedError("IllegalArgumentError")) {
  [IllegalArgumentErrorTypeId] = IllegalArgumentErrorTypeId;
  constructor(message) {
    super({
      message
    });
  }
};
var ExceededCapacityErrorTypeId = "~effect/Cause/ExceededCapacityError";
var ExceededCapacityError = class extends (/* @__PURE__ */ TaggedError("ExceededCapacityError")) {
  [ExceededCapacityErrorTypeId] = ExceededCapacityErrorTypeId;
  constructor(message) {
    super({
      message
    });
  }
};
var AsyncFiberErrorTypeId = "~effect/Cause/AsyncFiberError";
var AsyncFiberError = class extends (/* @__PURE__ */ TaggedError("AsyncFiberError")) {
  [AsyncFiberErrorTypeId] = AsyncFiberErrorTypeId;
  constructor(fiber2) {
    super({
      message: "An asynchronous Effect was executed with Effect.runSync",
      fiber: fiber2
    });
  }
};
var UnknownErrorTypeId = "~effect/Cause/UnknownError";
var UnknownError = class extends (/* @__PURE__ */ TaggedError("UnknownError")) {
  [UnknownErrorTypeId] = UnknownErrorTypeId;
  constructor(cause, message) {
    super({
      message,
      cause
    });
  }
};
var ConsoleRef = /* @__PURE__ */ Reference("effect/Console/CurrentConsole", {
  defaultValue: () => globalThis.console
});
var logLevelToOrder = (level) => {
  switch (level) {
    case "All":
      return Number.MIN_SAFE_INTEGER;
    case "Fatal":
      return 5e4;
    case "Error":
      return 4e4;
    case "Warn":
      return 3e4;
    case "Info":
      return 2e4;
    case "Debug":
      return 1e4;
    case "Trace":
      return 0;
    case "None":
      return Number.MAX_SAFE_INTEGER;
  }
};
var LogLevelOrder = /* @__PURE__ */ mapInput(Number2, logLevelToOrder);
var isLogLevelGreaterThan = /* @__PURE__ */ isGreaterThan(LogLevelOrder);
var CurrentLoggers = /* @__PURE__ */ Reference("effect/Loggers/CurrentLoggers", {
  defaultValue: () => /* @__PURE__ */ new Set([defaultLogger, tracerLogger])
});
var LogToStderr = /* @__PURE__ */ Reference("effect/Logger/LogToStderr", {
  defaultValue: constFalse
});
var LoggerTypeId = "~effect/Logger";
var LoggerProto = {
  [LoggerTypeId]: {
    _Message: identity,
    _Output: identity
  },
  pipe() {
    return pipeArguments(this, arguments);
  }
};
var loggerMake = (log) => {
  const self2 = Object.create(LoggerProto);
  self2.log = log;
  return self2;
};
var formatLabel = (key) => key.replace(/[\s="]/g, "_");
var formatLogSpan = (self2, now) => {
  const label = formatLabel(self2[0]);
  return `${label}=${now - self2[1]}ms`;
};
var logWithLevel = (level) => (...message) => {
  let cause = void 0;
  for (let i = 0, len = message.length; i < len; i++) {
    const msg = message[i];
    if (isCause(msg)) {
      if (cause) {
        ;
        message.splice(i, 1);
      } else {
        message = message.slice(0, i).concat(message.slice(i + 1));
      }
      cause = cause ? causeFromReasons(cause.reasons.concat(msg.reasons)) : msg;
      i--;
    }
  }
  if (cause === void 0) {
    cause = causeEmpty;
  }
  return withFiber((fiber2) => {
    const logLevel = level ?? fiber2.currentLogLevel;
    if (isLogLevelGreaterThan(fiber2.minimumLogLevel, logLevel)) {
      return void_;
    }
    const clock = fiber2.getRef(ClockRef);
    const loggers = fiber2.getRef(CurrentLoggers);
    if (loggers.size > 0) {
      const date = new Date(clock.currentTimeMillisUnsafe());
      for (const logger of loggers) {
        logger.log({
          cause,
          fiber: fiber2,
          date,
          logLevel,
          message
        });
      }
    }
    return void_;
  });
};
var colors = {
  bold: "1",
  red: "31",
  green: "32",
  yellow: "33",
  blue: "34",
  cyan: "36",
  white: "37",
  gray: "90",
  black: "30",
  bgBrightRed: "101"
};
var logLevelColors = {
  None: [],
  All: [],
  Trace: [colors.gray],
  Debug: [colors.blue],
  Info: [colors.green],
  Warn: [colors.yellow],
  Error: [colors.red],
  Fatal: [colors.bgBrightRed, colors.black]
};
var defaultDateFormat = (date) => `${date.getHours().toString().padStart(2, "0")}:${date.getMinutes().toString().padStart(2, "0")}:${date.getSeconds().toString().padStart(2, "0")}.${date.getMilliseconds().toString().padStart(3, "0")}`;
var defaultLogger = /* @__PURE__ */ loggerMake(({
  cause,
  date,
  fiber: fiber2,
  logLevel,
  message
}) => {
  const message_ = Array.isArray(message) ? message.slice() : [message];
  if (cause.reasons.length > 0) {
    message_.push(causePretty(cause));
  }
  const now = date.getTime();
  const spans = fiber2.getRef(CurrentLogSpans);
  let spanString = "";
  for (const span of spans) {
    spanString += ` ${formatLogSpan(span, now)}`;
  }
  const annotations = fiber2.getRef(CurrentLogAnnotations);
  if (Object.keys(annotations).length > 0) {
    message_.push(annotations);
  }
  const console2 = fiber2.getRef(ConsoleRef);
  const log = fiber2.getRef(LogToStderr) ? console2.error : console2.log;
  log(`[${defaultDateFormat(date)}] ${logLevel.toUpperCase()} (#${fiber2.id})${spanString}:`, ...message_);
});
var tracerLogger = /* @__PURE__ */ loggerMake(({
  cause,
  fiber: fiber2,
  logLevel,
  message
}) => {
  const clock = fiber2.getRef(ClockRef);
  const annotations = fiber2.getRef(CurrentLogAnnotations);
  const span = fiber2.currentSpan;
  if (span === void 0 || span._tag === "ExternalSpan") return;
  const attributes = {};
  for (const [key, value] of Object.entries(annotations)) {
    attributes[key] = value;
  }
  attributes["effect.fiberId"] = fiber2.id;
  attributes["effect.logLevel"] = logLevel.toUpperCase();
  if (cause.reasons.length > 0) {
    attributes["effect.cause"] = causePretty(cause);
  }
  span.event(toStringUnknown(Array.isArray(message) && message.length === 1 ? message[0] : message), clock.currentTimeNanosUnsafe(), attributes);
});

// node_modules/effect/dist/Exit.js
var failCause2 = exitFailCause;
var fail4 = exitFail;
var void_2 = exitVoid;
var isSuccess3 = exitIsSuccess;

// node_modules/effect/dist/Deferred.js
var TypeId5 = "~effect/Deferred";
var DeferredProto = {
  [TypeId5]: {
    _A: identity,
    _E: identity
  },
  pipe() {
    return pipeArguments(this, arguments);
  }
};
var makeUnsafe2 = () => {
  const self2 = Object.create(DeferredProto);
  self2.resumes = void 0;
  self2.effect = void 0;
  return self2;
};
var _await = (self2) => callback((resume) => {
  if (self2.effect) return resume(self2.effect);
  self2.resumes ??= [];
  self2.resumes.push(resume);
  return sync(() => {
    const index = self2.resumes.indexOf(resume);
    self2.resumes.splice(index, 1);
  });
});
var completeWith = /* @__PURE__ */ dual(2, (self2, effect2) => sync(() => doneUnsafe(self2, effect2)));
var done2 = completeWith;
var doneUnsafe = (self2, effect2) => {
  if (self2.effect) return false;
  self2.effect = effect2;
  if (self2.resumes) {
    for (let i = 0; i < self2.resumes.length; i++) {
      self2.resumes[i](effect2);
    }
    self2.resumes = void 0;
  }
  return true;
};

// node_modules/effect/dist/References.js
var CurrentLogAnnotations2 = CurrentLogAnnotations;
var TracerTimingEnabled2 = TracerTimingEnabled;

// node_modules/effect/dist/Scope.js
var Scope = scopeTag;
var make5 = scopeMake;
var makeUnsafe3 = scopeMakeUnsafe;
var provide = provideScope;
var addFinalizer2 = scopeAddFinalizer;
var forkUnsafe2 = scopeForkUnsafe;
var close = scopeClose;

// node_modules/effect/dist/Layer.js
var TypeId6 = "~effect/Layer";
var MemoMapTypeId = "~effect/Layer/MemoMap";
var memoMapReuse = (entry, scope3) => {
  entry.observers++;
  return andThen(scopeAddFinalizerExit(scope3, (exit3) => entry.finalizer(exit3)), entry.effect);
};
var LayerProto = {
  [TypeId6]: {
    _ROut: identity,
    _E: identity,
    _RIn: identity
  },
  pipe() {
    return pipeArguments(this, arguments);
  }
};
var fromBuildUnsafe = (build) => {
  const self2 = Object.create(LayerProto);
  self2.build = build;
  return self2;
};
var fromBuild = (build) => fromBuildUnsafe((memoMap, scope3) => {
  const layerScope = forkUnsafe2(scope3);
  return onExit(build(memoMap, layerScope), (exit3) => exit3._tag === "Failure" ? close(layerScope, exit3) : void_);
});
var fromBuildMemo = (build) => {
  const self2 = fromBuild((memoMap, scope3) => memoMap.getOrElseMemoize(self2, scope3, build));
  return self2;
};
var memoMapBuild = (memoMap, layer2, scope3, build) => {
  const layerScope = makeUnsafe3();
  const deferred = makeUnsafe2();
  const entry = {
    observers: 1,
    effect: _await(deferred),
    finalizer: (exit3) => suspend(() => {
      entry.observers--;
      if (entry.observers === 0) {
        memoMap.map.delete(layer2);
        return close(layerScope, exit3);
      }
      return void_;
    })
  };
  memoMap.map.set(layer2, entry);
  return scopeAddFinalizerExit(scope3, entry.finalizer).pipe(flatMap2(() => build(memoMap, layerScope)), onExit((exit3) => {
    entry.effect = exit3;
    return done2(deferred, exit3);
  }));
};
var MemoMapImpl = class {
  get [MemoMapTypeId]() {
    return MemoMapTypeId;
  }
  parent;
  constructor(parent) {
    this.parent = parent;
  }
  map = /* @__PURE__ */ new Map();
  get(layer2, scope3) {
    const local = this.map.get(layer2);
    if (local) {
      return memoMapReuse(local, scope3);
    }
    return this.parent?.get(layer2, scope3);
  }
  getOrElseMemoize(layer2, scope3, build) {
    const existing = this.get(layer2, scope3);
    if (existing) {
      return existing;
    }
    return memoMapBuild(this, layer2, scope3, build);
  }
};
var makeMemoMapUnsafe = () => new MemoMapImpl();
var CurrentMemoMap = class extends (/* @__PURE__ */ Service()("effect/Layer/CurrentMemoMap")) {
  static getOrCreate = /* @__PURE__ */ getOrElse2(this, makeMemoMapUnsafe);
};
var buildWithMemoMap = /* @__PURE__ */ dual(3, (self2, memoMap, scope3) => provideService(map3(self2.build(memoMap, scope3), add(CurrentMemoMap, memoMap)), CurrentMemoMap, memoMap));
var succeed4 = function() {
  if (arguments.length === 1) {
    return (resource) => succeedContext(make2(arguments[0], resource));
  }
  return succeedContext(make2(arguments[0], arguments[1]));
};
var succeedContext = (context3) => fromBuildUnsafe(constant(succeed3(context3)));
var effect = function() {
  if (arguments.length === 1) {
    return (effect2) => effectImpl(arguments[0], effect2);
  }
  return effectImpl(arguments[0], arguments[1]);
};
var effectImpl = (service3, effect2) => effectContext(map3(effect2, (value) => make2(service3, value)));
var effectContext = (effect2) => fromBuildMemo((_, scope3) => provide(effect2, scope3));
var mergeAllEffect = (layers, memoMap, scope3) => {
  const parentScope = forkUnsafe2(scope3, "parallel");
  return forEach(layers, (layer2) => layer2.build(memoMap, forkUnsafe2(parentScope, "sequential")), {
    concurrency: layers.length
  }).pipe(map3((context3) => mergeAll(...context3)));
};
var mergeAll2 = (...layers) => fromBuild((memoMap, scope3) => mergeAllEffect(layers, memoMap, scope3));
var provideWith = (self2, that, f) => fromBuild((memoMap, scope3) => flatMap2(Array.isArray(that) ? mergeAllEffect(that, memoMap, scope3) : that.build(memoMap, scope3), (context3) => self2.build(memoMap, scope3).pipe(provideContext(context3), map3((merged) => f(merged, context3)))));
var provide2 = /* @__PURE__ */ dual(2, (self2, that) => provideWith(self2, that, identity));

// node_modules/effect/dist/Cause.js
var isFailReason2 = isFailReason;
var die2 = causeDie;
var map4 = causeMap;
var findError2 = findError;
var isDone2 = isDone;
var done3 = done;

// node_modules/effect/dist/Data.js
var Error3 = Error2;
var TaggedError2 = TaggedError;

// node_modules/effect/dist/Clock.js
var Clock = ClockRef;

// node_modules/effect/dist/Pull.js
var catchDone = /* @__PURE__ */ dual(2, (effect2, f) => catchCauseFilter(effect2, filterDoneLeftover, (l) => f(l)));
var isDoneCause = (cause) => cause.reasons.some(isDoneFailure);
var isDoneFailure = (failure) => failure._tag === "Fail" && isDone2(failure.error);
var filterDoneLeftover = /* @__PURE__ */ composePassthrough(findError2, (e) => isDone2(e) ? succeed2(e.value) : fail2(e));

// node_modules/effect/dist/Effect.js
var isEffect2 = isEffect;
var all2 = all;
var forEach2 = forEach;
var tryPromise2 = tryPromise;
var succeed5 = succeed3;
var succeedNone2 = succeedNone;
var succeedSome2 = succeedSome;
var suspend2 = suspend;
var sync2 = sync;
var void_3 = void_;
var gen2 = gen;
var fail5 = fail3;
var failCause3 = failCause;
var failCauseSync2 = failCauseSync;
var die3 = die;
var try_2 = try_;
var withFiber2 = withFiber;
var fromResult2 = fromResult;
var flatMap3 = flatMap2;
var flatten2 = flatten;
var tap2 = tap;
var exit2 = exit;
var map5 = map3;
var as2 = as;
var catch_2 = catch_;
var catchTag2 = catchTag;
var catchCause2 = catchCause;
var catchDefect2 = catchDefect;
var catchIf2 = catchIf;
var mapError3 = mapError2;
var orDie2 = orDie;
var filterOrFail2 = filterOrFail;
var provideContext2 = provideContext;
var serviceOption2 = serviceOption;
var scoped2 = scoped;
var acquireRelease2 = acquireRelease;
var addFinalizer3 = addFinalizer;
var ensuring2 = ensuring;
var onExit2 = onExit;
var uninterruptibleMask2 = uninterruptibleMask;
var forever2 = forever;
var makeSpanScoped2 = makeSpanScoped;
var useSpan2 = useSpan;
var withSpan2 = withSpan;
var runFork2 = runFork;
var runForkWith2 = runForkWith;
var runCallbackWith2 = runCallbackWith;
var runCallback2 = runCallback;
var runPromise2 = runPromise;
var runPromiseWith2 = runPromiseWith;
var runPromiseExit2 = runPromiseExit;
var runPromiseExitWith2 = runPromiseExitWith;
var runSync2 = runSync;
var runSyncWith2 = runSyncWith;
var runSyncExit2 = runSyncExit;
var runSyncExitWith2 = runSyncExitWith;
var fnUntraced2 = fnUntraced;
var logInfo = /* @__PURE__ */ logWithLevel("Info");
var logDebug = /* @__PURE__ */ logWithLevel("Debug");
var annotateLogs = /* @__PURE__ */ dual((args2) => isEffect2(args2[0]), (effect2, ...args2) => updateService(effect2, CurrentLogAnnotations2, (annotations) => {
  const newAnnotations = {
    ...annotations
  };
  if (args2.length === 1) {
    Object.assign(newAnnotations, args2[0]);
  } else {
    newAnnotations[args2[0]] = args2[1];
  }
  return newAnnotations;
}));
var mapEager2 = mapEager;
var mapErrorEager2 = mapErrorEager;
var flatMapEager2 = flatMapEager;
var fnUntracedEager2 = fnUntracedEager;

// node_modules/effect/dist/internal/record.js
function set(self2, key, value) {
  if (key === "__proto__") {
    Object.defineProperty(self2, key, {
      value,
      writable: true,
      enumerable: true,
      configurable: true
    });
  } else {
    self2[key] = value;
  }
  return self2;
}

// node_modules/effect/dist/internal/schema/annotations.js
function resolve(ast) {
  return ast.checks ? ast.checks[ast.checks.length - 1].annotations : ast.annotations;
}
function resolveAt(key) {
  return (ast) => resolve(ast)?.[key];
}
var resolveIdentifier = /* @__PURE__ */ resolveAt("identifier");
var resolveBrands = /* @__PURE__ */ resolveAt("brands");
var getExpected = /* @__PURE__ */ memoize((ast) => {
  const identifier3 = resolveIdentifier(ast);
  if (typeof identifier3 === "string") return identifier3;
  return ast.getExpected(getExpected);
});

// node_modules/effect/dist/SchemaIssue.js
var TypeId7 = "~effect/SchemaIssue/Issue";
function isIssue(u) {
  return hasProperty(u, TypeId7);
}
var Base = class {
  [TypeId7] = TypeId7;
  toString() {
    return defaultFormatter(this);
  }
};
var Filter = class extends Base {
  _tag = "Filter";
  /**
   * The input value that caused the issue.
   */
  actual;
  /**
   * The filter that failed.
   */
  filter;
  /**
   * The issue that occurred.
   */
  issue;
  constructor(actual, filter5, issue) {
    super();
    this.actual = actual;
    this.filter = filter5;
    this.issue = issue;
  }
};
var Encoding = class extends Base {
  _tag = "Encoding";
  /**
   * The schema that caused the issue.
   */
  ast;
  /**
   * The input value that caused the issue.
   */
  actual;
  /**
   * The issue that occurred.
   */
  issue;
  constructor(ast, actual, issue) {
    super();
    this.ast = ast;
    this.actual = actual;
    this.issue = issue;
  }
};
var Pointer = class extends Base {
  _tag = "Pointer";
  /**
   * The path to the location in the input that caused the issue.
   */
  path;
  /**
   * The issue that occurred.
   */
  issue;
  constructor(path, issue) {
    super();
    this.path = path;
    this.issue = issue;
  }
};
var MissingKey = class extends Base {
  _tag = "MissingKey";
  /**
   * The metadata for the issue.
   */
  annotations;
  constructor(annotations) {
    super();
    this.annotations = annotations;
  }
};
var UnexpectedKey = class extends Base {
  _tag = "UnexpectedKey";
  /**
   * The schema that caused the issue.
   */
  ast;
  /**
   * The input value that caused the issue.
   */
  actual;
  constructor(ast, actual) {
    super();
    this.ast = ast;
    this.actual = actual;
  }
};
var Composite = class extends Base {
  _tag = "Composite";
  /**
   * The schema that caused the issue.
   */
  ast;
  /**
   * The input value that caused the issue.
   */
  actual;
  /**
   * The issues that occurred.
   */
  issues;
  constructor(ast, actual, issues) {
    super();
    this.ast = ast;
    this.actual = actual;
    this.issues = issues;
  }
};
var InvalidType = class extends Base {
  _tag = "InvalidType";
  /**
   * The schema that caused the issue.
   */
  ast;
  /**
   * The input value that caused the issue.
   */
  actual;
  constructor(ast, actual) {
    super();
    this.ast = ast;
    this.actual = actual;
  }
};
var InvalidValue = class extends Base {
  _tag = "InvalidValue";
  /**
   * The value that caused the issue.
   */
  actual;
  /**
   * The metadata for the issue.
   */
  annotations;
  constructor(actual, annotations) {
    super();
    this.actual = actual;
    this.annotations = annotations;
  }
};
var AnyOf = class extends Base {
  _tag = "AnyOf";
  /**
   * The schema that caused the issue.
   */
  ast;
  /**
   * The input value that caused the issue.
   */
  actual;
  /**
   * The issues that occurred.
   */
  issues;
  constructor(ast, actual, issues) {
    super();
    this.ast = ast;
    this.actual = actual;
    this.issues = issues;
  }
};
var OneOf = class extends Base {
  _tag = "OneOf";
  /**
   * The schema that caused the issue.
   */
  ast;
  /**
   * The input value that caused the issue.
   */
  actual;
  /**
   * The schemas that were successful.
   */
  successes;
  constructor(ast, actual, successes) {
    super();
    this.ast = ast;
    this.actual = actual;
    this.successes = successes;
  }
};
function makeFilterIssue(input, entry) {
  if (isIssue(entry)) {
    return entry;
  }
  if (typeof entry === "string") {
    return new InvalidValue(some2(input), {
      message: entry
    });
  }
  const inner = typeof entry.issue === "string" ? new InvalidValue(some2(input), {
    message: entry.issue
  }) : entry.issue;
  return new Pointer(entry.path, inner);
}
function makeSingle(input, out) {
  if (out === void 0) {
    return void 0;
  }
  if (typeof out === "boolean") {
    return out ? void 0 : new InvalidValue(some2(input));
  }
  return makeFilterIssue(input, out);
}
function make6(input, ast, out) {
  if (Array.isArray(out)) {
    if (isReadonlyArrayNonEmpty(out)) {
      if (out.length === 1) {
        return makeFilterIssue(input, out[0]);
      }
      return new Composite(ast, some2(input), map2(out, (entry) => makeFilterIssue(input, entry)));
    }
    return void 0;
  }
  return makeSingle(input, out);
}
var defaultLeafHook = (issue) => {
  const message = findMessage(issue);
  if (message !== void 0) return message;
  switch (issue._tag) {
    case "InvalidType":
      return getExpectedMessage(getExpected(issue.ast), formatOption(issue.actual));
    case "InvalidValue":
      return `Invalid data ${formatOption(issue.actual)}`;
    case "MissingKey":
      return "Missing key";
    case "UnexpectedKey":
      return `Unexpected key with value ${format(issue.actual)}`;
    case "Forbidden":
      return "Forbidden operation";
    case "OneOf":
      return `Expected exactly one member to match the input ${format(issue.actual)}`;
  }
};
var defaultCheckHook = (issue) => {
  return findMessage(issue.issue) ?? findMessage(issue);
};
function getExpectedMessage(expected, actual) {
  return `Expected ${expected}, got ${actual}`;
}
function toDefaultIssues(issue, path, leafHook, checkHook) {
  switch (issue._tag) {
    case "Filter": {
      const message = checkHook(issue);
      if (message !== void 0) {
        return [{
          path,
          message
        }];
      }
      switch (issue.issue._tag) {
        case "InvalidValue":
          return [{
            path,
            message: getExpectedMessage(formatCheck(issue.filter), format(issue.actual))
          }];
        default:
          return toDefaultIssues(issue.issue, path, leafHook, checkHook);
      }
    }
    case "Encoding":
      return toDefaultIssues(issue.issue, path, leafHook, checkHook);
    case "Pointer":
      return toDefaultIssues(issue.issue, [...path, ...issue.path], leafHook, checkHook);
    case "Composite":
      return issue.issues.flatMap((issue2) => toDefaultIssues(issue2, path, leafHook, checkHook));
    case "AnyOf": {
      const message = findMessage(issue);
      if (issue.issues.length === 0) {
        if (message !== void 0) return [{
          path,
          message
        }];
        const expected = getExpectedMessage(getExpected(issue.ast), format(issue.actual));
        return [{
          path,
          message: expected
        }];
      }
      return issue.issues.flatMap((issue2) => toDefaultIssues(issue2, path, leafHook, checkHook));
    }
    default:
      return [{
        path,
        message: leafHook(issue)
      }];
  }
}
function formatCheck(check2) {
  const expected = check2.annotations?.expected;
  if (typeof expected === "string") return expected;
  switch (check2._tag) {
    case "Filter":
      return "<filter>";
    case "FilterGroup":
      return check2.checks.map((check3) => formatCheck(check3)).join(" & ");
  }
}
function makeFormatterDefault() {
  return (issue) => toDefaultIssues(issue, [], defaultLeafHook, defaultCheckHook).map(formatDefaultIssue).join("\n");
}
var defaultFormatter = /* @__PURE__ */ makeFormatterDefault();
function formatDefaultIssue(issue) {
  let out = issue.message;
  if (issue.path && issue.path.length > 0) {
    const path = formatPath(issue.path);
    out += `
  at ${path}`;
  }
  return out;
}
function findMessage(issue) {
  switch (issue._tag) {
    case "InvalidType":
    case "OneOf":
    case "Composite":
    case "AnyOf":
      return getMessageAnnotation(issue.ast.annotations);
    case "InvalidValue":
    case "Forbidden":
      return getMessageAnnotation(issue.annotations);
    case "MissingKey":
      return getMessageAnnotation(issue.annotations, "messageMissingKey");
    case "UnexpectedKey":
      return getMessageAnnotation(issue.ast.annotations, "messageUnexpectedKey");
    case "Filter":
      return getMessageAnnotation(issue.filter.annotations);
    case "Encoding":
      return findMessage(issue.issue);
  }
}
function getMessageAnnotation(annotations, type = "message") {
  const message = annotations?.[type];
  if (typeof message === "string") return message;
}
function formatOption(actual) {
  if (isNone2(actual)) return "no value provided";
  return format(actual.value);
}

// node_modules/effect/dist/internal/schema/cause.js
function getSchemaIssue(cause) {
  let issue;
  for (const reason of cause.reasons) {
    if (!isFailReason2(reason) || !isIssue(reason.error)) {
      return void 0;
    }
    issue ??= reason.error;
  }
  return issue;
}
function getSchemaIssueOrThrow(cause, message) {
  const issue = getSchemaIssue(cause);
  if (issue === void 0) {
    throw new Error(message, {
      cause
    });
  }
  return issue;
}

// node_modules/effect/dist/Encoding.js
var EncodingErrorTypeId = "~effect/encoding/EncodingError";
var EncodingError = class extends (/* @__PURE__ */ TaggedError2("EncodingError")) {
  /**
   * Marks this value as an encoding or decoding error for runtime guards.
   *
   * **When to use**
   *
   * Use to identify `EncodingError` instances through `isEncodingError`.
   *
   * @since 4.0.0
   */
  [EncodingErrorTypeId] = EncodingErrorTypeId;
};
var encodeBase64 = (input) => typeof input === "string" ? base64EncodeUint8Array(encoder.encode(input)) : base64EncodeUint8Array(input);
var decodeBase64 = (str) => {
  const stripped = stripCrlf(str);
  const length = stripped.length;
  if (length % 4 !== 0) {
    return fail2(new EncodingError({
      kind: "Decode",
      module: "Base64",
      input: stripped,
      message: `Length must be a multiple of 4, but is ${length}`
    }));
  }
  const index = stripped.indexOf("=");
  if (index !== -1 && (index < length - 2 || index === length - 2 && stripped[length - 1] !== "=")) {
    return fail2(new EncodingError({
      kind: "Decode",
      module: "Base64",
      input: stripped,
      message: `Found a '=' character, but it is not at the end`
    }));
  }
  try {
    const missingOctets = stripped.endsWith("==") ? 2 : stripped.endsWith("=") ? 1 : 0;
    const result2 = new Uint8Array(3 * (length / 4) - missingOctets);
    for (let i = 0, j = 0; i < length; i += 4, j += 3) {
      const buffer2 = getBase64Code(stripped.charCodeAt(i)) << 18 | getBase64Code(stripped.charCodeAt(i + 1)) << 12 | getBase64Code(stripped.charCodeAt(i + 2)) << 6 | getBase64Code(stripped.charCodeAt(i + 3));
      result2[j] = buffer2 >> 16;
      result2[j + 1] = buffer2 >> 8 & 255;
      result2[j + 2] = buffer2 & 255;
    }
    return succeed2(result2);
  } catch (e) {
    return fail2(new EncodingError({
      kind: "Decode",
      module: "Base64",
      input: stripped,
      message: e instanceof Error ? e.message : "Invalid input"
    }));
  }
};
var encoder = /* @__PURE__ */ new TextEncoder();
var stripCrlf = (str) => str.replace(/[\n\r]/g, "");
var base64EncodeUint8Array = (bytes) => {
  const length = bytes.length;
  let result2 = "";
  let i;
  for (i = 2; i < length; i += 3) {
    result2 += base64abc[bytes[i - 2] >> 2];
    result2 += base64abc[(bytes[i - 2] & 3) << 4 | bytes[i - 1] >> 4];
    result2 += base64abc[(bytes[i - 1] & 15) << 2 | bytes[i] >> 6];
    result2 += base64abc[bytes[i] & 63];
  }
  if (i === length + 1) {
    result2 += base64abc[bytes[i - 2] >> 2];
    result2 += base64abc[(bytes[i - 2] & 3) << 4];
    result2 += "==";
  }
  if (i === length) {
    result2 += base64abc[bytes[i - 2] >> 2];
    result2 += base64abc[(bytes[i - 2] & 3) << 4 | bytes[i - 1] >> 4];
    result2 += base64abc[(bytes[i - 1] & 15) << 2];
    result2 += "=";
  }
  return result2;
};
function getBase64Code(charCode) {
  if (charCode >= base64codes.length) {
    throw new TypeError(`Invalid character ${String.fromCharCode(charCode)}`);
  }
  const code = base64codes[charCode];
  if (code === 255) {
    throw new TypeError(`Invalid character ${String.fromCharCode(charCode)}`);
  }
  return code;
}
var base64abc = ["A", "B", "C", "D", "E", "F", "G", "H", "I", "J", "K", "L", "M", "N", "O", "P", "Q", "R", "S", "T", "U", "V", "W", "X", "Y", "Z", "a", "b", "c", "d", "e", "f", "g", "h", "i", "j", "k", "l", "m", "n", "o", "p", "q", "r", "s", "t", "u", "v", "w", "x", "y", "z", "0", "1", "2", "3", "4", "5", "6", "7", "8", "9", "+", "/"];
var base64codes = [255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 62, 255, 255, 255, 63, 52, 53, 54, 55, 56, 57, 58, 59, 60, 61, 255, 255, 255, 0, 255, 255, 255, 0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19, 20, 21, 22, 23, 24, 25, 255, 255, 255, 255, 255, 255, 26, 27, 28, 29, 30, 31, 32, 33, 34, 35, 36, 37, 38, 39, 40, 41, 42, 43, 44, 45, 46, 47, 48, 49, 50, 51];

// node_modules/effect/dist/SchemaGetter.js
var Getter = class _Getter extends Class {
  run;
  constructor(run2) {
    super();
    this.run = run2;
  }
  map(f) {
    return new _Getter((oe, options) => this.run(oe, options).pipe(mapEager2(map(f))));
  }
  compose(other) {
    if (isPassthrough(this)) {
      return other;
    }
    if (isPassthrough(other)) {
      return this;
    }
    return new _Getter((oe, options) => this.run(oe, options).pipe(flatMapEager2((ot) => other.run(ot, options))));
  }
};
var passthrough_ = /* @__PURE__ */ new Getter(succeed5);
function isPassthrough(getter) {
  return getter.run === passthrough_.run;
}
function passthrough() {
  return passthrough_;
}
function onSome(f) {
  return new Getter((oe, options) => isNone2(oe) ? succeedNone2 : f(oe.value, options));
}
function transform(f) {
  return transformOptional(map(f));
}
function transformOrFail(f) {
  return onSome((e, options) => f(e, options).pipe(mapEager2(some2)));
}
function transformOptional(f) {
  return new Getter((oe) => succeed5(f(oe)));
}
function withDefault(defaultValue) {
  return new Getter((o) => {
    const filtered = filter(o, isNotUndefined);
    return isSome2(filtered) ? succeed5(filtered) : mapEager2(defaultValue, some2);
  });
}
function String2() {
  return transform(globalThis.String);
}
function Number3() {
  return transform(globalThis.Number);
}
function Date3() {
  return transform((u) => new globalThis.Date(u));
}
function encodeBase642() {
  return transform(encodeBase64);
}
function decodeBase642() {
  return transformOrFail((input) => mapErrorEager2(fromResult2(decodeBase64(input)), (e) => new InvalidValue(some2(input), {
    message: e.message
  })));
}

// node_modules/effect/dist/SchemaTransformation.js
var TypeId8 = "~effect/SchemaTransformation/Transformation";
var Transformation = class _Transformation {
  [TypeId8] = TypeId8;
  _tag = "Transformation";
  decode;
  encode;
  constructor(decode, encode) {
    this.decode = decode;
    this.encode = encode;
  }
  flip() {
    return new _Transformation(this.encode, this.decode);
  }
  compose(other) {
    return new _Transformation(this.decode.compose(other.decode), other.encode.compose(this.encode));
  }
};
function isTransformation(u) {
  return hasProperty(u, TypeId8);
}
var make7 = (options) => {
  if (isTransformation(options)) {
    return options;
  }
  return new Transformation(options.decode, options.encode);
};
function transformOrFail2(options) {
  return new Transformation(transformOrFail(options.decode), transformOrFail(options.encode));
}
function transform2(options) {
  return new Transformation(transform(options.decode), transform(options.encode));
}
var passthrough_2 = /* @__PURE__ */ new Transformation(/* @__PURE__ */ passthrough(), /* @__PURE__ */ passthrough());
function passthrough2() {
  return passthrough_2;
}
var numberFromString = /* @__PURE__ */ new Transformation(/* @__PURE__ */ Number3(), /* @__PURE__ */ String2());
var dateFromString = /* @__PURE__ */ new Transformation(/* @__PURE__ */ Date3(), /* @__PURE__ */ transform(formatDate));
var isJsonError = (input) => isObject(input) && typeof input["message"] === "string";
var decodeJsonError = (input) => {
  const hasCause = Object.hasOwn(input, "cause");
  const err = hasCause ? new Error(input.message, {
    cause: decodeDefect(input.cause)
  }) : new Error(input.message);
  if (typeof input.name === "string" && input.name !== "Error") err.name = input.name;
  if (typeof input.stack === "string") err.stack = input.stack;
  return err;
};
var encodeUnknownAsJson = (input) => {
  try {
    const json = formatJson(input);
    return json === void 0 ? format(input) : JSON.parse(json);
  } catch {
    return format(input);
  }
};
var encodeJsonError = (input, options, encodeDefect) => {
  const encoded = {
    name: input.name,
    message: typeof input.message === "string" ? input.message : ""
  };
  if (options?.includeStack && typeof input.stack === "string") {
    encoded.stack = input.stack;
  }
  if (!options?.excludeCause && input.cause !== void 0) {
    encoded.cause = encodeDefect(input.cause);
  }
  return encoded;
};
var makeEncodeDefect = (options) => {
  const seen = /* @__PURE__ */ new WeakSet();
  const encode = (input) => {
    if (isError(input)) {
      if (seen.has(input)) {
        return "[Circular]";
      }
      seen.add(input);
      const encoded = encodeJsonError(input, options, encode);
      seen.delete(input);
      return encoded;
    }
    return encodeUnknownAsJson(input);
  };
  return encode;
};
var decodeDefect = (input) => isJsonError(input) ? decodeJsonError(input) : input;
var defectFromJson = (options) => transform2({
  decode: decodeDefect,
  encode: makeEncodeDefect(options)
});
var urlFromString = /* @__PURE__ */ transformOrFail2({
  decode: (s) => URL.canParse(s) ? succeed5(new URL(s)) : fail5(new InvalidValue(some2(s), {
    message: `Invalid URL string: ${s}`
  })),
  encode: (url) => succeed5(url.href)
});
var uint8ArrayFromBase64String = /* @__PURE__ */ new Transformation(/* @__PURE__ */ decodeBase642(), /* @__PURE__ */ encodeBase642());

// node_modules/effect/dist/SchemaAST.js
function makeGuard(tag2) {
  return (ast) => ast._tag === tag2;
}
var isDeclaration = /* @__PURE__ */ makeGuard("Declaration");
var isNever2 = /* @__PURE__ */ makeGuard("Never");
var isLiteral = /* @__PURE__ */ makeGuard("Literal");
var isUniqueSymbol = /* @__PURE__ */ makeGuard("UniqueSymbol");
var isArrays = /* @__PURE__ */ makeGuard("Arrays");
var isObjects = /* @__PURE__ */ makeGuard("Objects");
var Link = class {
  to;
  transformation;
  constructor(to, transformation) {
    this.to = to;
    this.transformation = transformation;
  }
};
var defaultParseOptions = {};
var Context = class {
  isOptional;
  isMutable;
  /** Used for constructor default values (e.g. `withConstructorDefault` API) */
  defaultValue;
  annotations;
  constructor(isOptional2, isMutable, defaultValue = void 0, annotations = void 0) {
    this.isOptional = isOptional2;
    this.isMutable = isMutable;
    this.defaultValue = defaultValue;
    this.annotations = annotations;
  }
};
var TypeId9 = "~effect/Schema";
var Base2 = class {
  [TypeId9] = TypeId9;
  annotations;
  checks;
  encoding;
  context;
  constructor(annotations = void 0, checks = void 0, encoding = void 0, context3 = void 0) {
    this.annotations = annotations;
    this.checks = checks;
    this.encoding = encoding;
    this.context = context3;
  }
  toString() {
    return `<${this._tag}>`;
  }
};
var Declaration = class _Declaration extends Base2 {
  _tag = "Declaration";
  typeParameters;
  run;
  encodingChecks;
  constructor(typeParameters, run2, annotations, checks, encoding, context3, encodingChecks) {
    super(annotations, checks, encoding, context3);
    this.typeParameters = typeParameters;
    this.run = run2;
    this.encodingChecks = encodingChecks;
  }
  /** @internal */
  getParser() {
    const run2 = this.run(this.typeParameters);
    return (oinput, options) => {
      if (isNone2(oinput)) return succeedNone2;
      return mapEager2(run2(oinput.value, this, options), some2);
    };
  }
  _rebuild(recur2, checks, encodingChecks) {
    const tps = mapOrSame(this.typeParameters, recur2);
    return tps === this.typeParameters && checks === this.checks && encodingChecks === this.encodingChecks ? this : new _Declaration(tps, this.run, this.annotations, checks, void 0, this.context, encodingChecks);
  }
  /** @internal */
  recur(recur2) {
    return this._rebuild(recur2, this.checks, this.encodingChecks);
  }
  /** @internal */
  flip(recur2) {
    return this._rebuild(recur2, this.encodingChecks, this.checks);
  }
  /** @internal */
  getExpected() {
    const expected = this.annotations?.expected;
    if (typeof expected === "string") return expected;
    return "<Declaration>";
  }
};
var Null = class extends Base2 {
  _tag = "Null";
  /** @internal */
  getParser() {
    return fromConst(this, null);
  }
  /** @internal */
  getExpected() {
    return "null";
  }
};
var null_ = /* @__PURE__ */ new Null();
var Undefined = class extends Base2 {
  _tag = "Undefined";
  /** @internal */
  getParser() {
    return fromConst(this, void 0);
  }
  /** @internal */
  toCodecJson() {
    return replaceEncoding(this, [undefinedToNull]);
  }
  /** @internal */
  getExpected() {
    return "undefined";
  }
};
var undefinedToNull = /* @__PURE__ */ new Link(null_, /* @__PURE__ */ new Transformation(/* @__PURE__ */ transform(() => void 0), /* @__PURE__ */ transform(() => null)));
var undefined_2 = /* @__PURE__ */ new Undefined();
var Unknown = class extends Base2 {
  _tag = "Unknown";
  /** @internal */
  getParser() {
    return fromRefinement(this, isUnknown);
  }
  /** @internal */
  getExpected() {
    return "unknown";
  }
};
var unknown = /* @__PURE__ */ new Unknown();
var Literal = class extends Base2 {
  _tag = "Literal";
  literal;
  constructor(literal2, annotations, checks, encoding, context3) {
    super(annotations, checks, encoding, context3);
    if (typeof literal2 === "number" && !globalThis.Number.isFinite(literal2)) {
      throw new Error(`A numeric literal must be finite, got ${format(literal2)}`);
    }
    this.literal = literal2;
  }
  /** @internal */
  getParser() {
    return fromConst(this, this.literal);
  }
  /** @internal */
  matchPart(s, _options) {
    return s === globalThis.String(this.literal) ? this.literal : void 0;
  }
  /** @internal */
  toCodecJson() {
    return typeof this.literal === "bigint" ? literalToString(this) : this;
  }
  /** @internal */
  toCodecStringTree() {
    return typeof this.literal === "string" ? this : literalToString(this);
  }
  /** @internal */
  getExpected() {
    return typeof this.literal === "string" ? JSON.stringify(this.literal) : globalThis.String(this.literal);
  }
};
function literalToString(ast) {
  const literalAsString = globalThis.String(ast.literal);
  return replaceEncoding(ast, [new Link(new Literal(literalAsString), new Transformation(transform(() => ast.literal), transform(() => literalAsString)))]);
}
var String3 = class extends Base2 {
  _tag = "String";
  /** @internal */
  getParser() {
    return fromRefinement(this, isString);
  }
  /** @internal */
  matchPart(s, options) {
    return applyTemplateLiteralPartChecks(this, s, options);
  }
  /** @internal */
  getExpected() {
    return "string";
  }
};
var string2 = /* @__PURE__ */ new String3();
var Number4 = class extends Base2 {
  _tag = "Number";
  /** @internal */
  getParser() {
    return fromRefinement(this, isNumber);
  }
  /** @internal */
  matchKey(s, options) {
    return this._match(isStringNumberRegExp, s, options);
  }
  /** @internal */
  matchPart(s, options) {
    return this._match(isStringFiniteRegExp, s, options);
  }
  _match(regexp, s, options) {
    return regexp.test(s) ? applyTemplateLiteralPartChecks(this, globalThis.Number(s), options) : void 0;
  }
  /** @internal */
  toCodecJson() {
    if (this.checks && (hasCheck(this.checks, "isFinite") || hasCheck(this.checks, "isInt"))) {
      return this;
    }
    return replaceEncoding(this, [numberToJson]);
  }
  /** @internal */
  toCodecStringTree() {
    if (this.checks && (hasCheck(this.checks, "isFinite") || hasCheck(this.checks, "isInt"))) {
      return replaceEncoding(this, [finiteToString]);
    }
    return replaceEncoding(this, [numberToString]);
  }
  /** @internal */
  getExpected() {
    return "number";
  }
};
function hasCheck(checks, tag2) {
  return checks.some((c) => {
    switch (c._tag) {
      case "Filter":
        return c.annotations?.meta?._tag === tag2;
      case "FilterGroup":
        return hasCheck(c.checks, tag2);
    }
  });
}
var number2 = /* @__PURE__ */ new Number4();
var Boolean = class extends Base2 {
  _tag = "Boolean";
  /** @internal */
  getParser() {
    return fromRefinement(this, isBoolean);
  }
  /** @internal */
  getExpected() {
    return "boolean";
  }
};
var boolean = /* @__PURE__ */ new Boolean();
var Arrays = class _Arrays extends Base2 {
  _tag = "Arrays";
  isMutable;
  elements;
  rest;
  encodingChecks;
  constructor(isMutable, elements, rest, annotations, checks, encoding, context3, encodingChecks) {
    super(annotations, checks, encoding, context3);
    this.isMutable = isMutable;
    this.elements = elements;
    this.rest = rest;
    this.encodingChecks = encodingChecks;
    const i = elements.findIndex(isOptional);
    if (i !== -1 && (elements.slice(i + 1).some((e) => !isOptional(e)) || rest.length > 1)) {
      throw new Error("A required element cannot follow an optional element. ts(1257)");
    }
    if (rest.length > 1 && rest.slice(1).some(isOptional)) {
      throw new Error("An optional element cannot follow a rest element. ts(1266)");
    }
  }
  /** @internal */
  getParser(recur2) {
    const ast = this;
    const elements = ast.elements.map((ast2) => ({
      ast: ast2,
      parser: recur2(ast2)
    }));
    const rest = ast.rest.map((ast2) => ({
      ast: ast2,
      parser: recur2(ast2)
    }));
    const elementLen = elements.length;
    const [head, ...tail] = rest;
    const tailLen = tail.length;
    function getParser(tailThreshold, index) {
      if (index < elementLen) {
        return elements[index];
      } else if (index >= tailThreshold) {
        return tail[index - tailThreshold];
      }
      return head;
    }
    return fnUntracedEager2(function* (oinput, options) {
      if (oinput._tag === "None") {
        return oinput;
      }
      const input = oinput.value;
      if (!Array.isArray(input)) {
        return yield* fail5(new InvalidType(ast, oinput));
      }
      const len = input.length;
      const state = {
        ast,
        getParser,
        oinput,
        len,
        tailThreshold: resolveTailThreshold(len, elementLen, tailLen),
        output: new globalThis.Array(len),
        issues: void 0,
        options
      };
      const concurrency = resolveConcurrency(options?.concurrency);
      const eff = parseArray(state, input, {
        concurrency: concurrency?.concurrency,
        end: ast.rest.length === 0 ? elementLen : Math.max(len, elementLen + tailLen)
      });
      if (eff) yield* eff;
      if (ast.rest.length === 0 && len > elementLen) {
        for (let i = elementLen; i <= len - 1; i++) {
          const issue = new Pointer([i], new UnexpectedKey(ast, input[i]));
          if (options.errors === "all") {
            if (state.issues) state.issues.push(issue);
            else state.issues = [issue];
          } else {
            return yield* fail5(new Composite(ast, oinput, [issue]));
          }
        }
      }
      if (state.issues) {
        return yield* fail5(new Composite(ast, oinput, state.issues));
      }
      return some2(state.output);
    });
  }
  _rebuild(recur2, checks, encodingChecks) {
    const elements = mapOrSame(this.elements, recur2);
    const rest = mapOrSame(this.rest, recur2);
    return elements === this.elements && rest === this.rest && checks === this.checks && encodingChecks === this.encodingChecks ? this : new _Arrays(this.isMutable, elements, rest, this.annotations, checks, void 0, this.context, encodingChecks);
  }
  /** @internal */
  recur(recur2) {
    return this._rebuild(recur2, this.checks, this.encodingChecks);
  }
  /** @internal */
  flip(recur2) {
    return this._rebuild(recur2, this.encodingChecks, this.checks);
  }
  /** @internal */
  getExpected() {
    return "array";
  }
};
var parseArray = /* @__PURE__ */ iterateEager()({
  onItem(s, item, i) {
    const value = i < s.len ? some2(item) : none2();
    return s.getParser(s.tailThreshold, i).parser(value, s.options);
  },
  step(s, _, exit3, i) {
    if (exit3._tag === "Failure") {
      return wrapPropertyKeyIssue(s, s.ast, i, exit3);
    } else if (exit3.value._tag === "Some") {
      s.output[i] = exit3.value.value;
    } else {
      const p = s.getParser(s.tailThreshold, i);
      if (isOptional(p.ast)) return;
      const issue = new Pointer([i], new MissingKey(p.ast.context?.annotations));
      if (s.options.errors === "all") {
        if (s.issues) s.issues.push(issue);
        else s.issues = [issue];
      } else {
        return fail4(new Composite(s.ast, s.oinput, [issue]));
      }
    }
  }
});
function resolveTailThreshold(inputLen, elementLen, tailLen) {
  return Math.max(elementLen, inputLen - tailLen);
}
var resolveConcurrency = (value) => {
  value = value === "unbounded" ? Infinity : value ?? 1;
  return value > 1 ? {
    concurrency: value
  } : void 0;
};
var wrapPropertyKeyIssue = (s, ast, key, exit3) => {
  if (exit3.cause.reasons.length === 0) {
    return exit3;
  }
  const issue = getSchemaIssue(exit3.cause);
  if (issue === void 0) {
    return failCause2(map4(exit3.cause, (issue2) => new Composite(ast, s.oinput, [new Pointer([key], issue2)])));
  }
  const pointer = new Pointer([key], issue);
  if (s.options.errors === "all") {
    if (s.issues) s.issues.push(pointer);
    else s.issues = [pointer];
  } else {
    return fail4(new Composite(ast, s.oinput, [pointer]));
  }
};
var FINITE_PATTERN = "[+-]?\\d*\\.?\\d+(?:[Ee][+-]?\\d+)?";
function getIndexSignatureKeys(input, parameter2, options = defaultParseOptions) {
  let stringKeys;
  let symbolKeys;
  function go(parameter3) {
    switch (parameter3._tag) {
      case "String":
      case "TemplateLiteral":
        return (stringKeys ??= Object.keys(input)).filter((k) => parameter3.matchPart(k, options) !== void 0);
      case "Number":
        return (stringKeys ??= Object.keys(input)).filter((k) => parameter3.matchKey(k, options) !== void 0);
      case "Symbol":
        return (symbolKeys ??= Object.getOwnPropertySymbols(input)).filter((k) => parameter3.matchKey(k, options) !== void 0);
      case "Union":
        return [...new Set(parameter3.types.flatMap(go))];
      default:
        return [];
    }
  }
  return go(parameterFromPropertyKey(toEncoded(parameter2)));
}
var PropertySignature = class {
  name;
  type;
  constructor(name, type) {
    this.name = name;
    this.type = type;
  }
};
var KeyValueCombiner = class _KeyValueCombiner {
  decode;
  encode;
  constructor(decode, encode) {
    this.decode = decode;
    this.encode = encode;
  }
  /** @internal */
  flip() {
    return new _KeyValueCombiner(this.encode, this.decode);
  }
};
function isIndexSignatureParameterSide(ast) {
  switch (ast._tag) {
    case "String":
    case "Number":
    case "Symbol":
    case "TemplateLiteral":
      return true;
    case "Union":
      return ast.types.every(isIndexSignatureParameterSide);
    default:
      return false;
  }
}
function isIndexSignatureParameter(ast) {
  return isIndexSignatureParameterSide(ast) && isIndexSignatureParameterSide(toEncoded(ast));
}
var IndexSignature = class {
  parameter;
  type;
  merge;
  constructor(parameter2, type, merge3) {
    if (!isIndexSignatureParameter(parameter2)) {
      throw new Error(`Invalid index signature parameter ${parameter2._tag}`);
    }
    this.parameter = parameter2;
    this.type = type;
    this.merge = merge3;
    if (isOptional(type) && !containsUndefined(type)) {
      throw new Error("Cannot use `Schema.optionalKey` with index signatures, use `Schema.optional` instead.");
    }
  }
};
var Objects = class _Objects extends Base2 {
  _tag = "Objects";
  propertySignatures;
  indexSignatures;
  encodingChecks;
  constructor(propertySignatures, indexSignatures, annotations, checks, encoding, context3, encodingChecks) {
    super(annotations, checks, encoding, context3);
    this.propertySignatures = propertySignatures;
    this.indexSignatures = indexSignatures;
    this.encodingChecks = encodingChecks;
    const duplicates = propertySignatures.map((ps) => ps.name).filter((name, i, arr) => arr.indexOf(name) !== i);
    if (duplicates.length > 0) {
      throw new Error(`Duplicate identifiers: ${JSON.stringify(duplicates)}. ts(2300)`);
    }
  }
  /** @internal */
  getParser(recur2) {
    const ast = this;
    const expectedKeys = [];
    const expectedKeysSet = /* @__PURE__ */ new Set();
    const properties = [];
    for (const ps of ast.propertySignatures) {
      expectedKeys.push(ps.name);
      expectedKeysSet.add(ps.name);
      properties.push({
        ps,
        parser: recur2(ps.type),
        name: ps.name,
        type: ps.type
      });
    }
    const indexCount = ast.indexSignatures.length;
    if (ast.propertySignatures.length === 0 && ast.indexSignatures.length === 0) {
      return fromRefinement(ast, isNotNullish);
    }
    const parseIndexes = indexCount > 0 ? iterateEager()({
      onItem: fnUntracedEager2(function* (s, [key, is2]) {
        const parserKey = recur2(parameterFromPropertyKey(is2.parameter));
        const effKey = parserKey(some2(key), s.options);
        const exitKey = effectIsExit(effKey) ? effKey : yield* exit2(effKey);
        if (exitKey._tag === "Failure") {
          const eff = wrapPropertyKeyIssue(s, ast, key, exitKey);
          if (eff) yield* eff;
          return;
        }
        const value = some2(s.input[key]);
        const parserValue = recur2(is2.type);
        const effValue = parserValue(value, s.options);
        const exitValue = effectIsExit(effValue) ? effValue : yield* exit2(effValue);
        if (exitValue._tag === "Failure") {
          const eff = wrapPropertyKeyIssue(s, ast, key, exitValue);
          if (eff) yield* eff;
          return;
        } else if (exitKey.value._tag === "Some" && exitValue.value._tag === "Some") {
          const k2 = exitKey.value.value;
          if (expectedKeysSet.has(key) || expectedKeysSet.has(k2)) {
            return;
          }
          const v2 = exitValue.value.value;
          if (is2.merge && is2.merge.decode && Object.hasOwn(s.out, k2)) {
            const [k, v] = is2.merge.decode.combine([k2, s.out[k2]], [k2, v2]);
            set(s.out, k, v);
          } else {
            set(s.out, k2, v2);
          }
        }
      }),
      step: (_s, _, exit3) => exit3._tag === "Failure" ? exit3 : void 0
    }) : void 0;
    return fnUntracedEager2(function* (oinput, options) {
      if (oinput._tag === "None") {
        return oinput;
      }
      const input = oinput.value;
      if (!(typeof input === "object" && input !== null && !Array.isArray(input))) {
        return yield* fail5(new InvalidType(ast, oinput));
      }
      const out = {};
      const state = {
        ast,
        oinput,
        input,
        out,
        issues: void 0,
        options
      };
      const errorsAllOption = options.errors === "all";
      const onExcessPropertyError = options.onExcessProperty === "error";
      const onExcessPropertyPreserve = options.onExcessProperty === "preserve";
      let inputKeys;
      if (ast.indexSignatures.length === 0 && (onExcessPropertyError || onExcessPropertyPreserve)) {
        inputKeys = Reflect.ownKeys(input);
        for (let i = 0; i < inputKeys.length; i++) {
          const key = inputKeys[i];
          if (!expectedKeysSet.has(key)) {
            if (onExcessPropertyError) {
              const issue = new Pointer([key], new UnexpectedKey(ast, input[key]));
              if (errorsAllOption) {
                if (state.issues) {
                  state.issues.push(issue);
                } else {
                  state.issues = [issue];
                }
                continue;
              } else {
                return yield* fail5(new Composite(ast, oinput, [issue]));
              }
            } else {
              set(out, key, input[key]);
            }
          }
        }
      }
      const concurrency = resolveConcurrency(options?.concurrency);
      const eff = parseProperties(state, properties, concurrency);
      if (eff) yield* eff;
      if (parseIndexes) {
        const keyPairs = empty2();
        for (let i = 0; i < indexCount; i++) {
          const is2 = ast.indexSignatures[i];
          const keys = getIndexSignatureKeys(input, is2.parameter, options);
          for (let j = 0; j < keys.length; j++) {
            const key = keys[j];
            keyPairs.push([key, is2]);
          }
        }
        const eff2 = parseIndexes(state, keyPairs, concurrency);
        if (eff2) yield* eff2;
      }
      if (state.issues) {
        return yield* fail5(new Composite(ast, oinput, state.issues));
      }
      if (options.propertyOrder === "original") {
        const keys = (inputKeys ?? Reflect.ownKeys(input)).concat(expectedKeys);
        const preserved = {};
        for (const key of keys) {
          if (Object.hasOwn(out, key)) {
            set(preserved, key, out[key]);
          }
        }
        return some2(preserved);
      }
      return some2(out);
    });
  }
  _rebuild(recur2, recurParameter, flipMerge, checks, encodingChecks) {
    const props = mapOrSame(this.propertySignatures, (ps) => {
      const t = recur2(ps.type);
      return t === ps.type ? ps : new PropertySignature(ps.name, t);
    });
    const indexes = mapOrSame(this.indexSignatures, (is2) => {
      const p = recurParameter(is2.parameter);
      const t = recur2(is2.type);
      const merge3 = flipMerge ? is2.merge?.flip() : is2.merge;
      return p === is2.parameter && t === is2.type && merge3 === is2.merge ? is2 : new IndexSignature(p, t, merge3);
    });
    return props === this.propertySignatures && indexes === this.indexSignatures && checks === this.checks && encodingChecks === this.encodingChecks ? this : new _Objects(props, indexes, this.annotations, checks, void 0, this.context, encodingChecks);
  }
  /** @internal */
  flip(recur2) {
    return this._rebuild(recur2, recur2, true, this.encodingChecks, this.checks);
  }
  /** @internal */
  recur(recur2, recurParameter = recur2) {
    return this._rebuild(recur2, recurParameter, false, this.checks, this.encodingChecks);
  }
  /** @internal */
  getExpected() {
    if (this.propertySignatures.length === 0 && this.indexSignatures.length === 0) return "object | array";
    return "object";
  }
};
var parseProperties = /* @__PURE__ */ iterateEager()({
  onItem(s, p) {
    const value = Object.hasOwn(s.input, p.name) ? some2(s.input[p.name]) : none2();
    return p.parser(value, s.options);
  },
  step(s, p, exit3) {
    if (exit3._tag === "Failure") {
      return wrapPropertyKeyIssue(s, s.ast, p.name, exit3);
    } else if (exit3.value._tag === "Some") {
      set(s.out, p.name, exit3.value.value);
    } else if (!isOptional(p.type)) {
      const issue = new Pointer([p.name], new MissingKey(p.type.context?.annotations));
      if (s.options.errors === "all") {
        if (s.issues) s.issues.push(issue);
        else s.issues = [issue];
        return;
      } else {
        return fail4(new Composite(s.ast, s.oinput, [issue]));
      }
    }
  }
});
function combineChecks(a, b) {
  if (!a) return b;
  if (!b) return a;
  return [...a, ...b];
}
function struct(fields, checks, annotations) {
  return new Objects(Reflect.ownKeys(fields).map((key) => {
    return new PropertySignature(key, fields[key].ast);
  }), [], annotations, checks);
}
function getAST(self2) {
  return self2.ast;
}
function tuple(elements, checks = void 0) {
  return new Arrays(false, elements.map((e) => e.ast), [], void 0, checks);
}
function union2(members, mode, checks) {
  return new Union(members.map(getAST), mode, void 0, checks);
}
function getCandidateTypes(ast) {
  switch (ast._tag) {
    case "Null":
      return ["null"];
    case "Undefined":
      return ["undefined"];
    case "String":
    case "TemplateLiteral":
      return ["string"];
    case "Number":
      return ["number"];
    case "Boolean":
      return ["boolean"];
    case "Symbol":
    case "UniqueSymbol":
      return ["symbol"];
    case "BigInt":
      return ["bigint"];
    case "Arrays":
      return ["array"];
    case "ObjectKeyword":
      return ["object", "array", "function"];
    case "Objects":
      return ast.propertySignatures.length || ast.indexSignatures.length ? ["object"] : ["object", "array"];
    case "Enum":
      return Array.from(new Set(ast.enums.map(([, v]) => typeof v)));
    case "Literal":
      return [typeof ast.literal];
    case "Union":
      return Array.from(new Set(ast.types.flatMap(getCandidateTypes)));
    default:
      return ["null", "undefined", "string", "number", "boolean", "symbol", "bigint", "object", "array", "function"];
  }
}
function collectSentinels(ast) {
  switch (ast._tag) {
    default:
      return [];
    case "Declaration": {
      const s = ast.annotations?.["~sentinels"];
      return Array.isArray(s) ? s : [];
    }
    case "Objects":
      return ast.propertySignatures.flatMap((ps) => {
        const type = ps.type;
        if (!isOptional(type)) {
          if (isLiteral(type)) {
            return [{
              key: ps.name,
              literal: type.literal
            }];
          }
          if (isUniqueSymbol(type)) {
            return [{
              key: ps.name,
              literal: type.symbol
            }];
          }
        }
        return [];
      });
    case "Arrays":
      return ast.elements.flatMap((e, i) => {
        return isLiteral(e) && !isOptional(e) ? [{
          key: i,
          literal: e.literal
        }] : [];
      });
    case "Suspend":
      return collectSentinels(ast.thunk());
  }
}
var candidateIndexCache = /* @__PURE__ */ new WeakMap();
function getIndex(types) {
  let idx = candidateIndexCache.get(types);
  if (idx) return idx;
  idx = {};
  for (const a of types) {
    const encoded = toEncoded(a);
    if (isNever2(encoded)) continue;
    const types2 = getCandidateTypes(encoded);
    const sentinels = collectSentinels(encoded);
    idx.byType ??= {};
    for (const t of types2) (idx.byType[t] ??= []).push(a);
    if (sentinels.length > 0) {
      idx.bySentinel ??= /* @__PURE__ */ new Map();
      for (const {
        key,
        literal: literal2
      } of sentinels) {
        let m = idx.bySentinel.get(key);
        if (!m) idx.bySentinel.set(key, m = /* @__PURE__ */ new Map());
        let arr = m.get(literal2);
        if (!arr) m.set(literal2, arr = []);
        arr.push(a);
      }
    } else {
      idx.otherwise ??= {};
      for (const t of types2) (idx.otherwise[t] ??= []).push(a);
    }
  }
  candidateIndexCache.set(types, idx);
  return idx;
}
function filterLiterals(input) {
  return (ast) => {
    const encoded = toEncoded(ast);
    return encoded._tag === "Literal" ? encoded.literal === input : encoded._tag === "UniqueSymbol" ? encoded.symbol === input : true;
  };
}
function getCandidates(input, types) {
  const idx = getIndex(types);
  const runtimeType = input === null ? "null" : Array.isArray(input) ? "array" : typeof input;
  if (idx.bySentinel) {
    const base = idx.otherwise?.[runtimeType] ?? [];
    if (runtimeType === "object" || runtimeType === "array") {
      for (const [k, m] of idx.bySentinel) {
        if (Object.hasOwn(input, k)) {
          const match6 = m.get(input[k]);
          if (match6) return [...match6, ...base].filter(filterLiterals(input));
        }
      }
    }
    return base;
  }
  return (idx.byType?.[runtimeType] ?? []).filter(filterLiterals(input));
}
var Union = class _Union extends Base2 {
  _tag = "Union";
  types;
  mode;
  encodingChecks;
  constructor(types, mode, annotations, checks, encoding, context3, encodingChecks) {
    super(annotations, checks, encoding, context3);
    this.types = types;
    this.mode = mode;
    this.encodingChecks = encodingChecks;
  }
  /** @internal */
  getParser(recur2) {
    const ast = this;
    return (oinput, options) => {
      if (oinput._tag === "None") {
        return succeed5(oinput);
      }
      const input = oinput.value;
      const candidates = getCandidates(input, ast.types);
      const state = {
        ast,
        recur: recur2,
        oinput,
        input,
        out: void 0,
        successes: [],
        issues: void 0,
        options
      };
      const concurrency = resolveConcurrency(options?.concurrency);
      const eff = parseUnion(state, candidates, concurrency);
      if (!eff) {
        return state.out ? succeed5(state.out) : fail5(new AnyOf(ast, input, state.issues ?? []));
      }
      return flatMap3(eff, (_) => {
        return state.out ? succeed5(state.out) : fail5(new AnyOf(ast, input, state.issues ?? []));
      });
    };
  }
  _rebuild(recur2, checks, encodingChecks) {
    const types = mapOrSame(this.types, recur2);
    return types === this.types && checks === this.checks && encodingChecks === this.encodingChecks ? this : new _Union(types, this.mode, this.annotations, checks, void 0, this.context, encodingChecks);
  }
  /** @internal */
  recur(recur2) {
    return this._rebuild(recur2, this.checks, this.encodingChecks);
  }
  /** @internal */
  flip(recur2) {
    return this._rebuild(recur2, this.encodingChecks, this.checks);
  }
  /** @internal */
  matchPart(s, options) {
    for (const type of this.types) {
      const out = type.matchPart(s, options);
      if (out !== void 0) return out;
    }
    return void 0;
  }
  /** @internal */
  getExpected(getExpected2) {
    const expected = this.annotations?.expected;
    if (typeof expected === "string") return expected;
    if (this.types.length === 0) return "never";
    const types = this.types.map((type) => {
      const encoded = toEncoded(type);
      switch (encoded._tag) {
        case "Arrays": {
          const literals = encoded.elements.filter(isLiteral);
          if (literals.length > 0) {
            return `${formatIsMutable(encoded.isMutable)}[ ${literals.map((e) => getExpected2(e) + formatIsOptional(e.context?.isOptional)).join(", ")}, ... ]`;
          }
          break;
        }
        case "Objects": {
          const literals = encoded.propertySignatures.filter((ps) => isLiteral(ps.type));
          if (literals.length > 0) {
            return `{ ${literals.map((ps) => `${formatIsMutable(ps.type.context?.isMutable)}${formatPropertyKey(ps.name)}${formatIsOptional(ps.type.context?.isOptional)}: ${getExpected2(ps.type)}`).join(", ")}, ... }`;
          }
          break;
        }
      }
      return getExpected2(encoded);
    });
    return Array.from(new Set(types)).join(" | ");
  }
};
var parseUnion = /* @__PURE__ */ iterateEager()({
  onItem(s, ast) {
    const parser = s.recur(ast);
    return parser(s.oinput, s.options);
  },
  step(s, candidate, exit3) {
    if (exit3._tag === "Failure") {
      const issue = getSchemaIssue(exit3.cause);
      if (issue === void 0) {
        return exit3;
      }
      if (s.issues) s.issues.push(issue);
      else s.issues = [issue];
    } else {
      if (s.out && s.ast.mode === "oneOf") {
        s.successes.push(candidate);
        return fail4(new OneOf(s.ast, s.input, s.successes));
      }
      s.out = exit3.value;
      s.successes.push(candidate);
      if (s.ast.mode === "anyOf") {
        return void_2;
      }
    }
  }
});
var nonFiniteLiterals = /* @__PURE__ */ new Union([/* @__PURE__ */ new Literal("Infinity"), /* @__PURE__ */ new Literal("-Infinity"), /* @__PURE__ */ new Literal("NaN")], "anyOf");
var numberToJson = /* @__PURE__ */ new Link(/* @__PURE__ */ new Union([number2, nonFiniteLiterals], "anyOf"), /* @__PURE__ */ new Transformation(/* @__PURE__ */ Number3(), /* @__PURE__ */ transform((n) => globalThis.Number.isFinite(n) ? n : globalThis.String(n))));
function formatIsMutable(isMutable) {
  return isMutable ? "" : "readonly ";
}
function formatIsOptional(isOptional2) {
  return isOptional2 ? "?" : "";
}
var Filter2 = class _Filter extends Class {
  _tag = "Filter";
  run;
  annotations;
  /**
   * Whether the parsing process should be aborted after this check has failed.
   */
  aborted;
  constructor(run2, annotations = void 0, aborted = false) {
    super();
    this.run = run2;
    this.annotations = annotations;
    this.aborted = aborted;
  }
  annotate(annotations) {
    return new _Filter(this.run, {
      ...this.annotations,
      ...annotations
    }, this.aborted);
  }
  abort() {
    return new _Filter(this.run, this.annotations, true);
  }
  and(other, annotations) {
    return new FilterGroup([this, other], annotations);
  }
};
var FilterGroup = class _FilterGroup extends Class {
  _tag = "FilterGroup";
  checks;
  annotations;
  constructor(checks, annotations = void 0) {
    super();
    this.checks = checks;
    this.annotations = annotations;
  }
  annotate(annotations) {
    return new _FilterGroup(this.checks, {
      ...this.annotations,
      ...annotations
    });
  }
  and(other, annotations) {
    return new _FilterGroup([this, other], annotations);
  }
};
function makeFilter(filter5, annotations, aborted = false) {
  return new Filter2((input, ast, options) => make6(input, ast, filter5(input, ast, options)), annotations, aborted);
}
function isPattern(regExp, annotations) {
  const source = regExp.source;
  return makeFilter((s) => regExp.test(s), {
    expected: `a string matching the RegExp ${source}`,
    meta: {
      _tag: "isPattern",
      regExp
    },
    arbitrary: {
      constraint: {
        patterns: [regExp.source]
      }
    },
    ...annotations
  });
}
function modifyOwnPropertyDescriptors(ast, f) {
  const d = Object.getOwnPropertyDescriptors(ast);
  f(d);
  return Object.create(Object.getPrototypeOf(ast), d);
}
function replaceEncoding(ast, encoding) {
  if (ast.encoding === encoding) {
    return ast;
  }
  return modifyOwnPropertyDescriptors(ast, (d) => {
    d.encoding.value = encoding;
  });
}
function replaceContext(ast, context3) {
  if (ast.context === context3) {
    return ast;
  }
  return modifyOwnPropertyDescriptors(ast, (d) => {
    d.context.value = context3;
  });
}
function annotate(ast, annotations) {
  if (ast.checks) {
    const last = ast.checks[ast.checks.length - 1];
    return replaceChecks(ast, append(ast.checks.slice(0, -1), last.annotate(annotations)));
  }
  return modifyOwnPropertyDescriptors(ast, (d) => {
    d.annotations.value = {
      ...d.annotations.value,
      ...annotations
    };
  });
}
function replaceChecks(ast, checks) {
  if (ast._tag === "Suspend" && checks !== void 0) {
    throw new Error("Cannot add checks to Suspend");
  }
  if (ast.checks === checks) {
    return ast;
  }
  return modifyOwnPropertyDescriptors(ast, (d) => {
    d.checks.value = checks;
  });
}
function appendChecks(ast, checks) {
  return replaceChecks(ast, combineChecks(ast.checks, checks));
}
function updateLastLink(encoding, f) {
  const links = encoding;
  const last = links[links.length - 1];
  const to = f(last.to);
  if (to !== last.to) {
    return append(encoding.slice(0, encoding.length - 1), new Link(to, last.transformation));
  }
  return encoding;
}
function applyToLastLink(f) {
  return (ast) => ast.encoding ? replaceEncoding(ast, updateLastLink(ast.encoding, f)) : ast;
}
function applyToSelfOrLastLinkEncoding(f) {
  function out(ast) {
    return ast.encoding ? replaceEncoding(ast, updateLastLink(ast.encoding, out)) : f(ast);
  }
  return memoize(out);
}
function appendTransformation(from, transformation, to) {
  const link3 = new Link(from, transformation);
  return replaceEncoding(to, to.encoding ? [...to.encoding, link3] : [link3]);
}
function brand(ast, brand3) {
  const existing = resolveBrands(ast);
  const brands = existing ? [...existing, brand3] : [brand3];
  return annotate(ast, {
    brands
  });
}
function mapOrSame(as3, f) {
  let changed = false;
  const out = new Array(as3.length);
  for (let i = 0; i < as3.length; i++) {
    const a = as3[i];
    const fa2 = f(a);
    if (fa2 !== a) {
      changed = true;
    }
    out[i] = fa2;
  }
  return changed ? out : as3;
}
function annotateKey(ast, annotations) {
  const context3 = ast.context ? new Context(ast.context.isOptional, ast.context.isMutable, ast.context.defaultValue, {
    ...ast.context.annotations,
    ...annotations
  }) : new Context(false, false, void 0, annotations);
  return replaceContext(ast, context3);
}
var optionalKeyLastLink = /* @__PURE__ */ applyToLastLink(optionalKey);
function optionalKey(ast) {
  const context3 = ast.context ? ast.context.isOptional === false ? new Context(true, ast.context.isMutable, ast.context.defaultValue, ast.context.annotations) : ast.context : new Context(true, false);
  return optionalKeyLastLink(replaceContext(ast, context3));
}
function withConstructorDefault(ast, defaultValue) {
  const transformation = new Transformation(withDefault(defaultValue), passthrough());
  const encoding = [new Link(unknown, transformation)];
  const context3 = ast.context ? new Context(ast.context.isOptional, ast.context.isMutable, encoding, ast.context.annotations) : new Context(false, false, encoding);
  return replaceContext(ast, context3);
}
function decodeTo(from, to, transformation) {
  return appendTransformation(from, transformation, to);
}
function parseParameter(ast) {
  const literals = [];
  const parameters = [];
  function go(ast2) {
    switch (ast2._tag) {
      case "Literal":
        if (isPropertyKey(ast2.literal)) {
          literals.push(ast2.literal);
        }
        return;
      case "UniqueSymbol":
        literals.push(ast2.symbol);
        return;
      case "Never":
        return;
      case "Union":
        for (let i = 0; i < ast2.types.length; i++) {
          go(ast2.types[i]);
        }
        return;
      default:
        parameters.push(ast2);
    }
  }
  go(ast);
  return {
    literals,
    parameters
  };
}
function record(key, value, keyValueCombiner) {
  const {
    literals,
    parameters: indexSignatures
  } = parseParameter(key);
  return new Objects(literals.map((literal2) => new PropertySignature(literal2, value)), indexSignatures.map((parameter2) => new IndexSignature(parameter2, value, keyValueCombiner)));
}
function isOptional(ast) {
  return ast.context?.isOptional ?? false;
}
var toType = /* @__PURE__ */ memoize((ast) => {
  if (ast.encoding) {
    return toType(replaceEncoding(ast, void 0));
  }
  const out = ast;
  const type = out.recur?.(toType) ?? out;
  const encodingChecks = type.encodingChecks;
  if (encodingChecks) {
    return modifyOwnPropertyDescriptors(type, (d) => {
      d.encodingChecks.value = void 0;
      if (type === ast) {
        d.checks.value = combineChecks(type.checks, encodingChecks);
      }
    });
  }
  return type;
});
var toEncoded = /* @__PURE__ */ memoize((ast) => {
  return toType(flip2(ast));
});
function flipEncoding(ast, encoding) {
  const links = encoding;
  const len = links.length;
  const last = links[len - 1];
  const ls = [new Link(flip2(replaceEncoding(ast, void 0)), links[0].transformation.flip())];
  for (let i = 1; i < len; i++) {
    ls.unshift(new Link(flip2(links[i - 1].to), links[i].transformation.flip()));
  }
  const to = flip2(last.to);
  if (to.encoding) {
    return replaceEncoding(to, [...to.encoding, ...ls]);
  } else {
    return replaceEncoding(to, ls);
  }
}
var flip2 = /* @__PURE__ */ memoize((ast) => {
  if (ast.encoding) {
    return flipEncoding(ast, ast.encoding);
  }
  const out = ast;
  return out.flip?.(flip2) ?? out.recur?.(flip2) ?? out;
});
function containsUndefined(ast) {
  switch (ast._tag) {
    case "Undefined":
      return true;
    case "Union":
      return ast.types.some(containsUndefined);
    default:
      return false;
  }
}
function fromConst(ast, value) {
  const succeed8 = succeedSome2(value);
  return (oinput) => {
    if (oinput._tag === "None") {
      return succeedNone2;
    }
    return oinput.value === value ? succeed8 : fail5(new InvalidType(ast, oinput));
  };
}
function fromRefinement(ast, refinement) {
  return (oinput) => {
    if (oinput._tag === "None") {
      return succeedNone2;
    }
    return refinement(oinput.value) ? succeed5(oinput) : fail5(new InvalidType(ast, oinput));
  };
}
function applyTemplateLiteralPartChecks(ast, value, options) {
  if (options?.disableChecks || ast.checks === void 0) return value;
  const issues = [];
  collectIssues(ast.checks, value, issues, ast, options);
  return issues.length === 0 ? value : void 0;
}
var parameterFromPropertyKey = /* @__PURE__ */ applyToSelfOrLastLinkEncoding((ast) => {
  switch (ast._tag) {
    default:
      return ast;
    case "Number":
      return ast.toCodecStringTree();
    case "Union":
      return ast.recur(parameterFromPropertyKey);
  }
});
var isStringFiniteRegExp = /* @__PURE__ */ new globalThis.RegExp(`^${FINITE_PATTERN}$`);
var isStringNumberRegExp = /* @__PURE__ */ new globalThis.RegExp(`(?:${FINITE_PATTERN}|Infinity|-Infinity|NaN)`);
function isStringFinite(annotations) {
  return isPattern(isStringFiniteRegExp, {
    expected: "a string representing a finite number",
    meta: {
      _tag: "isStringFinite",
      regExp: isStringFiniteRegExp
    },
    ...annotations
  });
}
var finiteString = /* @__PURE__ */ appendChecks(string2, [/* @__PURE__ */ isStringFinite()]);
var finiteToString = /* @__PURE__ */ new Link(finiteString, numberFromString);
var numberToString = /* @__PURE__ */ new Link(/* @__PURE__ */ new Union([finiteString, nonFiniteLiterals], "anyOf"), numberFromString);
var BIGINT_PATTERN = "-?\\d+";
var isStringBigIntRegExp = /* @__PURE__ */ new globalThis.RegExp(`^${BIGINT_PATTERN}$`);
var REGEXP_PATTERN = "Symbol\\((.*)\\)";
var isStringSymbolRegExp = /* @__PURE__ */ new globalThis.RegExp(`^${REGEXP_PATTERN}$`);
function collectIssues(checks, value, issues, ast, options) {
  for (let i = 0; i < checks.length; i++) {
    const check2 = checks[i];
    if (check2._tag === "FilterGroup") {
      collectIssues(check2.checks, value, issues, ast, options);
    } else {
      const issue = check2.run(value, ast, options);
      if (issue) {
        issues.push(new Filter(value, check2, issue));
        if (check2.aborted || options?.errors !== "all") {
          return;
        }
      }
    }
  }
}
var ClassTypeId = "~effect/Schema/Class";
var STRUCTURAL_ANNOTATION_KEY = "~structural";
function isJson(u) {
  const onPath = /* @__PURE__ */ new Set();
  const validated = /* @__PURE__ */ new Set();
  return recur2(u);
  function recur2(u2) {
    if (u2 === null || typeof u2 === "string" || typeof u2 === "boolean") {
      return true;
    }
    if (typeof u2 === "number") {
      return globalThis.Number.isFinite(u2);
    }
    if (typeof u2 !== "object" || u2 === void 0) {
      return false;
    }
    if (onPath.has(u2)) {
      return false;
    }
    if (validated.has(u2)) {
      return true;
    }
    onPath.add(u2);
    const ok = Array.isArray(u2) ? u2.every(recur2) : Object.keys(u2).every((key) => recur2(u2[key]));
    onPath.delete(u2);
    if (ok) {
      validated.add(u2);
    }
    return ok;
  }
}
var Json = /* @__PURE__ */ new Declaration([], () => (input, ast) => isJson(input) ? succeed5(input) : fail5(new InvalidType(ast, some2(input))), {
  typeConstructor: {
    _tag: "effect/Json"
  },
  generation: {
    runtime: `Schema.Json`,
    Type: `Schema.Json`
  },
  expected: "JSON value",
  toCodecJson: () => new Link(unknown, passthrough2()),
  toArbitrary: () => (fc) => fc.jsonValue()
});

// node_modules/effect/dist/PlatformError.js
var TypeId10 = "~effect/platform/PlatformError";
var BadArgument = class extends (/* @__PURE__ */ TaggedError2("BadArgument")) {
  /**
   * Formats the module, method, and optional description that rejected the argument.
   *
   * **When to use**
   *
   * Use to read the formatted error message for a rejected platform argument.
   *
   * @since 4.0.0
   */
  get message() {
    return `${this.module}.${this.method}${this.description ? `: ${this.description}` : ""}`;
  }
};
var SystemError = class extends Error3 {
  /**
   * Formats the normalized system error tag with operation and path details.
   *
   * **When to use**
   *
   * Use to read the formatted error message for a normalized system failure.
   *
   * @since 4.0.0
   */
  get message() {
    return `${this._tag}: ${this.module}.${this.method}${this.pathOrDescriptor !== void 0 ? ` (${this.pathOrDescriptor})` : ""}${this.description ? `: ${this.description}` : ""}`;
  }
};
var PlatformError = class extends (/* @__PURE__ */ TaggedError2("PlatformError")) {
  constructor(reason) {
    if ("cause" in reason) {
      super({
        reason,
        cause: reason.cause
      });
    } else {
      super({
        reason
      });
    }
  }
  /**
   * Marks this value as a platform error wrapper for runtime guards.
   *
   * **When to use**
   *
   * Use to identify `PlatformError` values through their runtime type marker.
   *
   * @since 4.0.0
   */
  [TypeId10] = TypeId10;
  get message() {
    return this.reason.message;
  }
};
var systemError = (options) => new PlatformError(new SystemError(options));
var badArgument = (options) => new PlatformError(new BadArgument(options));

// node_modules/effect/dist/Fiber.js
var TypeId11 = `~effect/Fiber/${version}`;
var await_ = fiberAwait;
var getCurrent = getCurrentFiber;
var runIn = fiberRunIn;

// node_modules/effect/dist/MutableList.js
var Empty = /* @__PURE__ */ Symbol.for("effect/MutableList/Empty");
var emptyBucket = () => ({
  array: [],
  mutable: true,
  offset: 0,
  next: void 0
});
var append2 = (self2, message) => {
  if (!self2.tail) {
    self2.head = self2.tail = emptyBucket();
  } else if (!self2.tail.mutable) {
    self2.tail.next = emptyBucket();
    self2.tail = self2.tail.next;
  }
  self2.tail.array.push(message);
  self2.length++;
};
var clear = (self2) => {
  self2.head = self2.tail = void 0;
  self2.length = 0;
};
var takeN = (self2, n) => {
  if (n <= 0 || !self2.head) return [];
  n = Math.min(n, self2.length);
  if (n === self2.length && self2.head?.offset === 0 && !self2.head.next) {
    const array3 = self2.head.array;
    clear(self2);
    return array3;
  }
  const array2 = new Array(n);
  let index = 0;
  let chunk = self2.head;
  while (chunk) {
    while (chunk.offset < chunk.array.length) {
      array2[index++] = chunk.array[chunk.offset];
      if (chunk.mutable) chunk.array[chunk.offset] = void 0;
      chunk.offset++;
      if (index === n) {
        self2.head = chunk;
        self2.length -= n;
        if (self2.length === 0) clear(self2);
        return array2;
      }
    }
    chunk = chunk.next;
  }
  clear(self2);
  return array2;
};
var take = (self2) => {
  if (!self2.head) return Empty;
  const message = self2.head.array[self2.head.offset];
  if (self2.head.mutable) self2.head.array[self2.head.offset] = void 0;
  self2.head.offset++;
  self2.length--;
  if (self2.head.offset === self2.head.array.length) {
    if (self2.head.next) {
      self2.head = self2.head.next;
    } else {
      clear(self2);
    }
  }
  return message;
};

// node_modules/effect/dist/MutableRef.js
var TypeId12 = "~effect/MutableRef";
var MutableRefProto = {
  [TypeId12]: TypeId12,
  ...PipeInspectableProto,
  toJSON() {
    return {
      _id: "MutableRef",
      current: toJson(this.current)
    };
  }
};
var make8 = (value) => {
  const ref = Object.create(MutableRefProto);
  ref.current = value;
  return ref;
};
var set2 = /* @__PURE__ */ dual(2, (self2, value) => {
  self2.current = value;
  return self2;
});

// node_modules/effect/dist/Queue.js
var TypeId13 = "~effect/Queue";
var EnqueueTypeId = "~effect/Queue/Enqueue";
var DequeueTypeId = "~effect/Queue/Dequeue";
var variance = {
  _A: identity,
  _E: identity
};
var QueueProto = {
  [TypeId13]: variance,
  [EnqueueTypeId]: variance,
  [DequeueTypeId]: variance,
  ...PipeInspectableProto,
  toJSON() {
    return {
      _id: "effect/Queue",
      state: this.state._tag,
      size: sizeUnsafe(this)
    };
  }
};
var takeAll2 = (self2) => takeBetween(self2, 1, Number.POSITIVE_INFINITY);
var takeBetween = (self2, min, max) => suspend(() => takeBetweenUnsafe(self2, min, max) ?? andThen(awaitTake(self2), takeBetween(self2, 1, max)));
var sizeUnsafe = (self2) => self2.state._tag === "Done" ? 0 : self2.messages.length;
var exitTrue = /* @__PURE__ */ exitSucceed(true);
var takeBetweenUnsafe = (self2, min, max) => {
  if (self2.state._tag === "Done") {
    return self2.state.exit;
  } else if (max <= 0 || min <= 0) {
    return exitSucceed([]);
  } else if (self2.capacity <= 0 && self2.state.offers.size > 0) {
    self2.capacity = 1;
    releaseCapacity(self2);
    self2.capacity = 0;
    const messages = [take(self2.messages)];
    releaseCapacity(self2);
    return exitSucceed(messages);
  }
  min = Math.min(min, self2.capacity || 1);
  if (min <= self2.messages.length) {
    const messages = takeN(self2.messages, max);
    releaseCapacity(self2);
    return exitSucceed(messages);
  }
};
var releaseCapacity = (self2) => {
  if (self2.state._tag === "Done") {
    return isDoneCause(self2.state.exit.cause);
  } else if (self2.state.offers.size === 0) {
    if (self2.state._tag === "Closing" && self2.messages.length === 0) {
      finalize(self2, self2.state.exit);
      return isDoneCause(self2.state.exit.cause);
    }
    return false;
  }
  let n = self2.capacity - self2.messages.length;
  for (const entry of self2.state.offers) {
    if (n === 0) break;
    else if (entry._tag === "Single") {
      append2(self2.messages, entry.message);
      n--;
      entry.resume(exitTrue);
      self2.state.offers.delete(entry);
    } else {
      for (; entry.offset < entry.remaining.length; entry.offset++) {
        if (n === 0) return false;
        append2(self2.messages, entry.remaining[entry.offset]);
        n--;
      }
      entry.resume(exitSucceed([]));
      self2.state.offers.delete(entry);
    }
  }
  return false;
};
var awaitTake = (self2) => callback((resume) => {
  if (self2.state._tag === "Done") {
    return resume(self2.state.exit);
  }
  self2.state.takers.add(resume);
  return sync(() => {
    if (self2.state._tag !== "Done") {
      self2.state.takers.delete(resume);
    }
  });
});
var finalize = (self2, exit3) => {
  if (self2.state._tag === "Done") {
    return;
  }
  const openState = self2.state;
  self2.state = {
    _tag: "Done",
    exit: exit3
  };
  for (const taker of openState.takers) {
    taker(exit3);
  }
  openState.takers.clear();
  for (const awaiter of openState.awaiters) {
    awaiter(exit3);
  }
  openState.awaiters.clear();
};

// node_modules/effect/dist/Semaphore.js
var SemaphoreImpl = class {
  waiters = /* @__PURE__ */ new Set();
  taken = 0;
  permits;
  constructor(permits) {
    this.permits = permits;
  }
  get free() {
    return this.permits - this.taken;
  }
  take(n) {
    const take3 = suspend(() => {
      if (this.free < n) {
        return callback((resume) => {
          if (this.free >= n) return resume(take3);
          const observer = () => {
            if (this.free < n) return;
            this.waiters.delete(observer);
            resume(take3);
          };
          this.waiters.add(observer);
          return sync(() => {
            this.waiters.delete(observer);
          });
        });
      }
      this.taken += n;
      return succeed3(n);
    });
    return take3;
  }
  updateTakenUnsafe(fiber2, f) {
    this.taken = f(this.taken);
    if (this.waiters.size > 0) {
      fiber2.currentDispatcher.scheduleTask(() => {
        const iter = this.waiters.values();
        let item = iter.next();
        while (item.done === false && this.free > 0) {
          item.value();
          item = iter.next();
        }
      }, 0);
    }
    return this.free;
  }
  updateTaken(f) {
    return withFiber((fiber2) => succeed3(this.updateTakenUnsafe(fiber2, f)));
  }
  resize(permits) {
    return withFiber((fiber2) => {
      this.permits = permits;
      if (this.free < 0) return void_;
      this.updateTakenUnsafe(fiber2, (taken) => taken);
      return void_;
    });
  }
  release(n) {
    return this.updateTaken((taken) => taken - n);
  }
  get releaseAll() {
    return this.updateTaken((_) => 0);
  }
  withPermits(n) {
    return (self2) => uninterruptibleMask((restore) => flatMap2(restore(this.take(n)), (permits) => onExitPrimitive(restore(self2), () => {
      this.updateTakenUnsafe(getCurrentFiber(), (taken) => taken - permits);
      return void 0;
    }, true)));
  }
  withPermit = /* @__PURE__ */ this.withPermits(1);
  withPermitsIfAvailable(n) {
    return (self2) => uninterruptibleMask((restore) => {
      if (this.free < n) return succeedNone;
      this.taken += n;
      return onExitPrimitive(restore(asSome(self2)), () => {
        this.updateTakenUnsafe(getCurrentFiber(), (taken) => taken - n);
        return void 0;
      }, true);
    });
  }
};
var make10 = (permits) => sync(() => new SemaphoreImpl(permits));

// node_modules/effect/dist/Channel.js
var TypeId14 = "~effect/Channel";
var ChannelProto = {
  [TypeId14]: {
    _Env: identity,
    _InErr: identity,
    _InElem: identity,
    _OutErr: identity,
    _OutElem: identity
  },
  pipe() {
    return pipeArguments(this, arguments);
  }
};
var fromTransform = (transform3) => {
  const self2 = Object.create(ChannelProto);
  self2.transform = (upstream, scope3) => catchCause2(transform3(upstream, scope3), (cause) => succeed5(failCause3(cause)));
  return self2;
};
var fromPull = (effect2) => fromTransform((_, __) => effect2);
var toTransform = (channel) => channel.transform;
var failCause5 = (cause) => fromPull(failCause3(cause));
var die4 = (defect) => failCause5(die2(defect));
var fromQueueArray = (queue) => fromPull(succeed5(takeAll2(queue)));
var unwrap = (channel) => fromTransform((upstream, scope3) => {
  let pull;
  return succeed5(suspend2(() => {
    if (pull) return pull;
    return channel.pipe(provide(scope3), flatMap3((channel2) => toTransform(channel2)(upstream, scope3)), flatMap3((pull_) => pull = pull_));
  }));
});

// node_modules/effect/dist/internal/stream.js
var TypeId15 = "~effect/Stream";
var streamVariance = {
  _R: identity,
  _E: identity,
  _A: identity
};
var StreamProto = {
  [TypeId15]: streamVariance,
  pipe() {
    return pipeArguments(this, arguments);
  }
};
var fromChannel = (channel) => {
  const self2 = Object.create(StreamProto);
  self2.channel = channel;
  return self2;
};

// node_modules/effect/dist/Sink.js
var TypeId16 = "~effect/Sink";
var endVoid = /* @__PURE__ */ succeed5([void 0]);
var sinkVariance = {
  _A: identity,
  _In: identity,
  _L: identity,
  _E: identity,
  _R: identity
};
var SinkProto = {
  [TypeId16]: sinkVariance,
  pipe() {
    return pipeArguments(this, arguments);
  }
};
var fromChannel2 = (channel) => fromTransform2((upstream, scope3) => toTransform(channel)(upstream, scope3).pipe(flatMap3(forever2({
  disableYield: true
})), catchDone(succeed5)));
var fromTransform2 = (transform3) => {
  const self2 = Object.create(SinkProto);
  self2.transform = transform3;
  return self2;
};
var toChannel = (self2) => fromTransform((upstream, scope3) => succeed5(flatMap3(self2.transform(upstream, scope3), done3)));
var forEach3 = (f) => forEachArray(forEach2((_) => f(_), {
  discard: true
}));
var forEachArray = (f) => fromTransform2((upstream) => upstream.pipe(flatMap3(f), forever2({
  disableYield: true
}), catchDone(() => endVoid)));
var unwrap2 = (effect2) => fromChannel2(unwrap(map5(effect2, toChannel)));

// node_modules/effect/dist/Stream.js
var fromChannel3 = fromChannel;
var fromPull2 = (pull) => fromChannel3(fromPull(pull));
var toChannel2 = (stream) => stream.channel;
var die5 = (defect) => fromChannel3(die4(defect));
var fromQueue = (queue) => fromChannel3(fromQueueArray(queue));
var unwrap3 = (effect2) => fromChannel3(unwrap(map5(effect2, toChannel2)));

// node_modules/effect/dist/FileSystem.js
var TypeId17 = "~effect/platform/FileSystem";
var Size = (bytes) => typeof bytes === "bigint" ? bytes : BigInt(bytes);
var bigint1024 = /* @__PURE__ */ BigInt(1024);
var bigintPiB = bigint1024 * bigint1024 * bigint1024 * bigint1024 * bigint1024;
var FileSystem = /* @__PURE__ */ Service("effect/platform/FileSystem");
var make12 = (impl2) => FileSystem.of({
  ...impl2,
  [TypeId17]: TypeId17,
  exists: (path) => pipe(impl2.access(path), as2(true), catchTag2("PlatformError", (e) => e.reason._tag === "NotFound" ? succeed5(false) : fail5(e))),
  readFileString: (path, encoding) => flatMap3(impl2.readFile(path), (_) => try_2({
    try: () => new TextDecoder(encoding).decode(_),
    catch: (cause) => badArgument({
      module: "FileSystem",
      method: "readFileString",
      description: "invalid encoding",
      cause
    })
  })),
  stream: fnUntraced2(function* (path, options) {
    const file = yield* impl2.open(path, {
      flag: "r"
    });
    if (options?.offset) {
      yield* file.seek(options.offset, "start");
    }
    const bytesToRead = options?.bytesToRead !== void 0 ? Size(options.bytesToRead) : void 0;
    let totalBytesRead = BigInt(0);
    const chunkSize = Size(options?.chunkSize ?? 64 * 1024);
    const readChunk = file.readAlloc(chunkSize);
    return fromPull2(succeed5(flatMap3(suspend2(() => {
      if (bytesToRead !== void 0 && bytesToRead <= totalBytesRead) {
        return done3();
      }
      return bytesToRead !== void 0 && bytesToRead - totalBytesRead < chunkSize ? file.readAlloc(bytesToRead - totalBytesRead) : readChunk;
    }), match({
      onNone: () => done3(),
      onSome: (buf) => {
        totalBytesRead += BigInt(buf.length);
        return succeed5(of(buf));
      }
    }))));
  }, unwrap3),
  sink: (path, options) => pipe(impl2.open(path, {
    flag: "w",
    ...options
  }), map5((file) => forEach3((_) => file.writeAll(_))), unwrap2),
  writeFileString: (path, data, options) => flatMap3(try_2({
    try: () => new TextEncoder().encode(data),
    catch: (cause) => badArgument({
      module: "FileSystem",
      method: "writeFileString",
      description: "could not encode string",
      cause
    })
  }), (_) => impl2.writeFile(path, _, options))
});

// src/services/MetatoolRuntime.ts
import { dirname, join } from "node:path";
import { exec } from "node:child_process";

// node_modules/effect/dist/Struct.js
var lambda = (f) => f;

// node_modules/effect/dist/SchemaParser.js
var recurDefaults = /* @__PURE__ */ memoize((ast) => {
  switch (ast._tag) {
    case "Declaration": {
      const getLink = ast.annotations?.[ClassTypeId];
      if (isFunction(getLink)) {
        const link3 = getLink(ast.typeParameters);
        const to = recurDefaults(link3.to);
        return replaceEncoding(ast, to === link3.to ? [link3] : [new Link(to, link3.transformation)]);
      }
      return ast;
    }
    case "Objects":
    case "Arrays":
      return ast.recur((ast2) => {
        const defaultValue = ast2.context?.defaultValue;
        if (defaultValue) {
          return replaceEncoding(recurDefaults(ast2), defaultValue);
        }
        return recurDefaults(ast2);
      });
    case "Suspend":
      return ast.recur(recurDefaults);
    default:
      return ast;
  }
});
function makeEffect(schema) {
  const ast = recurDefaults(toType(schema.ast));
  const parser = run(ast);
  return (input, options) => {
    return parser(input, options?.disableChecks ? options?.parseOptions ? {
      ...options.parseOptions,
      disableChecks: true
    } : {
      disableChecks: true
    } : options?.parseOptions);
  };
}
function makeOption(schema) {
  const parser = makeEffect(schema);
  return (input, options) => {
    const exit3 = runSyncExit2(parser(input, options));
    if (isSuccess3(exit3)) {
      return some2(exit3.value);
    }
    getSchemaIssueOrThrow(exit3.cause, "Option adapter can only return none for schema issues");
    return none2();
  };
}
function make13(schema) {
  const parser = makeEffect(schema);
  return (input, options) => {
    const exit3 = runSyncExit2(parser(input, options));
    if (isSuccess3(exit3)) {
      return exit3.value;
    }
    const issue = getSchemaIssueOrThrow(exit3.cause, "Constructor adapter can only throw schema issues");
    throw new Error(issue.toString(), {
      cause: issue
    });
  };
}
function decodeUnknownEffect(schema, options) {
  const parser = run(schema.ast);
  return options === void 0 ? parser : (input, overrideOptions) => parser(input, mergeParseOptions(options, overrideOptions));
}
var mergeParseOptions = (options, overrideOptions) => overrideOptions === void 0 ? options : {
  ...options,
  ...overrideOptions
};
function run(ast) {
  const parser = recur(ast);
  return (input, options) => flatMapEager2(parser(some2(input), options ?? defaultParseOptions), (oa2) => {
    if (oa2._tag === "None") {
      return fail5(new InvalidValue(oa2));
    }
    return succeed5(oa2.value);
  });
}
function mapSchemaIssueEffect(self2, f) {
  return catchCause2(self2, (cause) => failCauseSync2(() => map4(cause, f)));
}
var recur = /* @__PURE__ */ memoize((ast) => {
  let parser;
  const checks = ast.checks;
  const encoding = ast.encoding;
  const links = encoding;
  const len = links?.length ?? 0;
  const encodingChecks = ast.encodingChecks;
  const astOptions = (checks ? checks[checks.length - 1].annotations : ast.annotations)?.["parseOptions"];
  if (!ast.context && !encoding && !checks && !encodingChecks) {
    return (ou, options) => {
      parser ??= ast.getParser(recur);
      if (astOptions) {
        options = {
          ...options,
          ...astOptions
        };
      }
      return parser(ou, options);
    };
  }
  const isStructural = isArrays(ast) || isObjects(ast) || isDeclaration(ast) && ast.typeParameters.length > 0;
  const structuralChecks = checks && isStructural ? checks.filter((check2) => check2.annotations?.[STRUCTURAL_ANNOTATION_KEY]) : void 0;
  return (ou, options) => {
    if (astOptions) {
      options = {
        ...options,
        ...astOptions
      };
    }
    let srou;
    if (links) {
      for (let i = len - 1; i >= 0; i--) {
        const link3 = links[i];
        const to = link3.to;
        const parser2 = recur(to);
        srou = srou ? flatMapEager2(srou, (ou2) => parser2(ou2, options)) : parser2(ou, options);
        if (link3.transformation._tag === "Transformation") {
          const getter = link3.transformation.decode;
          srou = flatMapEager2(srou, (ou2) => getter.run(ou2, options));
        } else {
          srou = link3.transformation.decode(srou, options);
        }
      }
      srou = mapSchemaIssueEffect(srou, (issue) => new Encoding(ast, ou, issue));
    }
    parser ??= ast.getParser(recur);
    const parseLocal = (localOu) => {
      let sroa2 = parser(localOu, options);
      if (encodingChecks && !options?.disableChecks) {
        sroa2 = flatMapEager2(sroa2, (oa2) => {
          if (isSome2(localOu) && isSome2(oa2)) {
            const issues = [];
            collectIssues(encodingChecks, localOu.value, issues, ast, options);
            if (isArrayNonEmpty2(issues)) {
              return fail5(new Composite(ast, localOu, issues));
            }
          }
          return succeed5(oa2);
        });
      }
      if (checks && !options?.disableChecks) {
        if (options?.errors === "all" && structuralChecks && structuralChecks.length > 0 && isSome2(localOu)) {
          sroa2 = mapSchemaIssueEffect(sroa2, (issue) => {
            const issues = [];
            collectIssues(structuralChecks, localOu.value, issues, ast, options);
            const out = isArrayNonEmpty2(issues) ? issue._tag === "Composite" && issue.ast === ast ? new Composite(ast, issue.actual, [...issue.issues, ...issues]) : new Composite(ast, localOu, [issue, ...issues]) : issue;
            return out;
          });
        }
        sroa2 = flatMapEager2(sroa2, (oa2) => {
          if (isSome2(oa2)) {
            const value = oa2.value;
            const issues = [];
            collectIssues(checks, value, issues, ast, options);
            if (isArrayNonEmpty2(issues)) {
              return fail5(new Composite(ast, oa2, issues));
            }
          }
          return succeed5(oa2);
        });
      }
      return sroa2;
    };
    const sroa = srou ? flatMapEager2(srou, parseLocal) : parseLocal(ou);
    return sroa;
  };
});

// node_modules/effect/dist/internal/schema/schema.js
var TypeId18 = "~effect/Schema/Schema";
var SchemaProto = {
  [TypeId18]: TypeId18,
  pipe() {
    return pipeArguments(this, arguments);
  },
  annotate(annotations) {
    return this.rebuild(annotate(this.ast, annotations));
  },
  annotateKey(annotations) {
    return this.rebuild(annotateKey(this.ast, annotations));
  },
  check(...checks) {
    return this.rebuild(appendChecks(this.ast, checks));
  }
};
function make14(ast, options) {
  const self2 = Object.create(SchemaProto);
  if (options) {
    Object.assign(self2, options);
  }
  self2.ast = ast;
  self2.rebuild = (ast2) => make14(ast2, options);
  self2.makeEffect = (input, options2) => mapSchemaIssueEffect2(makeEffect(self2)(input, options2));
  self2.make = make13(self2);
  self2.makeOption = makeOption(self2);
  return self2;
}
var SchemaErrorTypeId = "~effect/Schema/SchemaError";
var SchemaError = class extends (/* @__PURE__ */ TaggedError2("SchemaError")) {
  [SchemaErrorTypeId] = SchemaErrorTypeId;
  constructor(issue) {
    super({
      issue
    });
  }
  get message() {
    return this.issue.toString();
  }
  toString() {
    return `SchemaError(${this.message})`;
  }
};
function mapSchemaIssueEffect2(self2) {
  return catchCause2(self2, (cause) => failCauseSync2(() => map4(cause, (issue) => new SchemaError(issue))));
}
function mapSchemaErrorEffect(self2) {
  return catchCause2(self2, (cause) => failCauseSync2(() => map4(cause, (error) => error.issue)));
}

// node_modules/effect/dist/Schema.js
var TypeId19 = TypeId18;
function declareConstructor() {
  return (typeParameters, run2, annotations) => {
    return make15(new Declaration(typeParameters.map(getAST), (typeParameters2) => run2(typeParameters2.map((ast) => make15(ast))), annotations));
  };
}
function declare(is2, annotations) {
  return declareConstructor()([], () => (input, ast) => is2(input) ? succeed5(input) : fail5(new InvalidType(ast, some2(input))), annotations);
}
function isSchemaError(u) {
  return hasProperty(u, SchemaErrorTypeId);
}
function decodeUnknownEffect2(schema, options) {
  const parser = decodeUnknownEffect(schema, options);
  return (input, options2) => {
    return mapSchemaIssueEffect2(parser(input, options2));
  };
}
function getSchemaErrorOrThrow(cause, message) {
  let schemaError;
  for (const reason of cause.reasons) {
    if (!isFailReason2(reason) || !isSchemaError(reason.error)) {
      throw new globalThis.Error(message, {
        cause
      });
    }
    schemaError ??= reason.error;
  }
  if (schemaError === void 0) {
    throw new globalThis.Error(message, {
      cause
    });
  }
  return schemaError;
}
function runSchemaErrorSync(self2) {
  const exit3 = runSyncExit2(self2);
  if (isSuccess3(exit3)) {
    return exit3.value;
  }
  throw getSchemaErrorOrThrow(exit3.cause, "Sync adapter can only throw schema errors");
}
function decodeUnknownSync(schema, options) {
  const parser = decodeUnknownEffect2(schema, options);
  return (input, options2) => {
    return runSchemaErrorSync(parser(input, options2));
  };
}
var make15 = make14;
function isSchema(u) {
  return hasProperty(u, TypeId19) && u[TypeId19] === TypeId19;
}
var optionalKey2 = /* @__PURE__ */ lambda((schema) => make15(optionalKey(schema.ast), {
  schema
}));
var optional = /* @__PURE__ */ lambda((self2) => optionalKey2(UndefinedOr(self2)));
function Literal2(literal2) {
  const out = make15(new Literal(literal2), {
    literal: literal2,
    transform(to) {
      return out.pipe(decodeTo2(Literal2(to), {
        decode: transform(() => to),
        encode: transform(() => literal2)
      }));
    }
  });
  return out;
}
var Unknown2 = /* @__PURE__ */ make15(unknown);
var Null2 = /* @__PURE__ */ make15(null_);
var Undefined2 = /* @__PURE__ */ make15(undefined_2);
var String4 = /* @__PURE__ */ make15(string2);
var Number5 = /* @__PURE__ */ make15(number2);
var Boolean2 = /* @__PURE__ */ make15(boolean);
function makeStruct(ast, fields) {
  return make15(ast, {
    fields,
    mapFields(f, options) {
      const fields2 = f(this.fields);
      return makeStruct(struct(fields2, options?.unsafePreserveChecks ? this.ast.checks : void 0), fields2);
    }
  });
}
function Struct(fields) {
  return makeStruct(struct(fields, void 0), fields);
}
function Record(key, value, options) {
  const keyValueCombiner = options?.keyValueCombiner?.decode || options?.keyValueCombiner?.encode ? new KeyValueCombiner(options.keyValueCombiner.decode, options.keyValueCombiner.encode) : void 0;
  return make15(record(key.ast, value.ast, keyValueCombiner), {
    key,
    value
  });
}
function makeTuple(ast, elements) {
  return make15(ast, {
    elements,
    mapElements(f, options) {
      const elements2 = f(this.elements);
      return makeTuple(tuple(elements2, options?.unsafePreserveChecks ? this.ast.checks : void 0), elements2);
    }
  });
}
function Tuple(elements) {
  return makeTuple(tuple(elements), elements);
}
var ArraySchema = /* @__PURE__ */ lambda((schema) => make15(new Arrays(false, [], [schema.ast]), {
  value: schema
}));
function makeUnion(ast, members) {
  return make15(ast, {
    members,
    mapMembers(f, options) {
      const members2 = f(this.members);
      return makeUnion(union2(members2, this.ast.mode, options?.unsafePreserveChecks ? this.ast.checks : void 0), members2);
    }
  });
}
function Union2(members, options) {
  return makeUnion(union2(members, options?.mode ?? "anyOf", void 0), members);
}
function Literals(literals) {
  const members = literals.map(Literal2);
  return make15(union2(members, "anyOf", void 0), {
    literals,
    members,
    mapMembers(f) {
      return Union2(f(this.members));
    },
    pick(literals2) {
      return Literals(literals2);
    },
    transform(to) {
      return Union2(members.map((member, index) => member.transform(to[index])));
    }
  });
}
var NullOr = /* @__PURE__ */ lambda((self2) => Union2([self2, Null2]));
var UndefinedOr = /* @__PURE__ */ lambda((self2) => Union2([self2, Undefined2]));
function check(...checks) {
  return (self2) => self2.check(...checks);
}
function brand2(identifier3) {
  return (schema) => make15(brand(schema.ast, identifier3), {
    schema,
    identifier: identifier3
  });
}
function decodeTo2(to, transformation) {
  return (from) => {
    return make15(decodeTo(from.ast, to.ast, transformation ? make7(transformation) : passthrough2()), {
      from,
      to
    });
  };
}
function withConstructorDefault2(defaultValue) {
  return (schema) => make15(withConstructorDefault(schema.ast, mapSchemaErrorEffect(defaultValue)), {
    schema
  });
}
function tag(literal2) {
  return Literal2(literal2).pipe(withConstructorDefault2(succeed5(literal2)));
}
function TaggedStruct(value, fields) {
  return Struct({
    _tag: tag(value),
    ...fields
  });
}
function instanceOf(constructor, annotations) {
  return declare((u) => u instanceof constructor, annotations);
}
function link() {
  return (encodeTo, transformation) => {
    return new Link(encodeTo.ast, make7(transformation));
  };
}
var makeFilter2 = makeFilter;
var isPattern2 = isPattern;
function isBase64(annotations) {
  const regExp = /^([0-9a-zA-Z+/]{4})*(([0-9a-zA-Z+/]{2}==)|([0-9a-zA-Z+/]{3}=))?$/;
  return isPattern2(regExp, {
    expected: "a base64 encoded string",
    meta: {
      _tag: "isBase64",
      regExp
    },
    ...annotations
  });
}
function isMinLength(minLength, annotations) {
  minLength = Math.max(0, Math.floor(minLength));
  return makeFilter2((input) => input.length >= minLength, {
    expected: `a value with a length of at least ${minLength}`,
    meta: {
      _tag: "isMinLength",
      minLength
    },
    [STRUCTURAL_ANNOTATION_KEY]: true,
    arbitrary: {
      constraint: {
        minLength
      }
    },
    ...annotations
  });
}
function isNonEmpty(annotations) {
  return isMinLength(1, annotations);
}
var NonEmptyString = /* @__PURE__ */ String4.check(/* @__PURE__ */ isNonEmpty());
var getErrorOptionsKey = (options) => (options?.includeStack === true ? 1 : 0) | (options?.excludeCause === true ? 2 : 0);
var getErrorOptions = (key) => {
  switch (key) {
    case 0:
      return void 0;
    case 1:
      return {
        includeStack: true
      };
    case 2:
      return {
        excludeCause: true
      };
    case 3:
      return {
        includeStack: true,
        excludeCause: true
      };
  }
};
var defectSchemaCache = [];
function Defect(options) {
  const key = getErrorOptionsKey(options);
  const cached2 = defectSchemaCache[key];
  if (cached2 !== void 0) {
    return cached2;
  }
  const schema = Json2.pipe(decodeTo2(Unknown2, defectFromJson(getErrorOptions(key))));
  defectSchemaCache[key] = schema;
  return schema;
}
var RegExp2 = /* @__PURE__ */ instanceOf(globalThis.RegExp, {
  typeConstructor: {
    _tag: "RegExp"
  },
  generation: {
    runtime: `Schema.RegExp`,
    Type: `globalThis.RegExp`
  },
  expected: "RegExp",
  toCodecJson: () => link()(Struct({
    source: String4,
    flags: String4
  }), transformOrFail2({
    decode: (e) => try_2({
      try: () => new globalThis.RegExp(e.source, e.flags),
      catch: (e2) => new InvalidValue(some2(e2), {
        message: globalThis.String(e2)
      })
    }),
    encode: (regExp) => succeed5({
      source: regExp.source,
      flags: regExp.flags
    })
  })),
  toArbitrary: () => (fc) => fc.tuple(fc.constantFrom(
    ".",
    ".*",
    "\\d+",
    "\\w+",
    "[a-z]+",
    "[A-Z]+",
    "[0-9]+",
    "^[a-zA-Z0-9]+$",
    "^\\d{4}-\\d{2}-\\d{2}$"
    // date pattern
  ), fc.uniqueArray(fc.constantFrom("g", "i", "m", "s", "u", "y"), {
    minLength: 0,
    maxLength: 6
  }).map((flags) => flags.join(""))).map(([source, flags]) => new globalThis.RegExp(source, flags)),
  toEquivalence: () => (a, b) => a.source === b.source && a.flags === b.flags
});
var URLString = /* @__PURE__ */ String4.annotate({
  expected: "a string that will be decoded as a URL"
});
var URL2 = /* @__PURE__ */ instanceOf(globalThis.URL, {
  typeConstructor: {
    _tag: "URL"
  },
  generation: {
    runtime: `Schema.URL`,
    Type: `globalThis.URL`
  },
  expected: "URL",
  toCodecJson: () => link()(URLString, urlFromString),
  toArbitrary: () => (fc) => fc.webUrl().map((s) => new globalThis.URL(s)),
  toEquivalence: () => (a, b) => a.toString() === b.toString()
});
function dateArbitraryConstraints(constraint, ordered, base, toDate) {
  const out = {
    ...base
  };
  delete out.valid;
  if (base?.valid || constraint?.valid) {
    out.noInvalidDate = true;
  }
  if (ordered?.minimum !== void 0) {
    const minimum = toDate === void 0 ? ordered.minimum : toDate(ordered.minimum);
    const nextMin = ordered.exclusiveMinimum ? new globalThis.Date(minimum.getTime() + 1) : minimum;
    if (out.min === void 0 || nextMin.getTime() > out.min.getTime()) {
      out.min = nextMin;
    }
  }
  if (ordered?.maximum !== void 0) {
    const maximum = toDate === void 0 ? ordered.maximum : toDate(ordered.maximum);
    const nextMax = ordered.exclusiveMaximum ? new globalThis.Date(maximum.getTime() - 1) : maximum;
    if (out.max === void 0 || nextMax.getTime() < out.max.getTime()) {
      out.max = nextMax;
    }
  }
  return out;
}
var DateString = /* @__PURE__ */ String4.annotate({
  expected: "a string in ISO 8601 format that will be decoded as a Date"
});
var Date4 = /* @__PURE__ */ instanceOf(globalThis.Date, {
  typeConstructor: {
    _tag: "Date"
  },
  generation: {
    runtime: `Schema.Date`,
    Type: `globalThis.Date`
  },
  expected: "Date",
  toCodecJson: () => link()(DateString, dateFromString),
  toArbitrary: () => (fc, ctx) => fc.date(dateArbitraryConstraints(ctx?.constraint, ctx?.constraint?.ordered?.order === Date2 ? ctx.constraint.ordered : void 0))
});
var File = /* @__PURE__ */ instanceOf(globalThis.File, {
  typeConstructor: {
    _tag: "File"
  },
  generation: {
    runtime: `Schema.File`,
    Type: `globalThis.File`
  },
  expected: "File",
  toCodecJson: () => link()(Struct({
    data: String4.check(isBase64()),
    type: String4,
    name: String4,
    lastModified: Number5
  }), transformOrFail2({
    decode: (e) => match3(decodeBase64(e.data), {
      onFailure: (error) => fail5(new InvalidValue(some2(e.data), {
        message: error.message
      })),
      onSuccess: (bytes) => {
        const buffer2 = new globalThis.Uint8Array(bytes);
        return succeed5(new globalThis.File([buffer2], e.name, {
          type: e.type,
          lastModified: e.lastModified
        }));
      }
    }),
    encode: (file) => tryPromise2({
      try: async () => {
        const bytes = new globalThis.Uint8Array(await file.arrayBuffer());
        return {
          data: encodeBase64(bytes),
          type: file.type,
          name: file.name,
          lastModified: file.lastModified
        };
      },
      catch: (e) => new InvalidValue(some2(file), {
        message: globalThis.String(e)
      })
    })
  }))
});
var FormData2 = /* @__PURE__ */ instanceOf(globalThis.FormData, {
  typeConstructor: {
    _tag: "FormData"
  },
  generation: {
    runtime: `Schema.FormData`,
    Type: `globalThis.FormData`
  },
  expected: "FormData",
  toCodecJson: () => link()(ArraySchema(Tuple([String4, Union2([Struct({
    _tag: tag("String"),
    value: String4
  }), Struct({
    _tag: tag("File"),
    value: File
  })])])), transformOrFail2({
    decode: (e) => {
      const out = new globalThis.FormData();
      for (const [key, entry] of e) {
        out.append(key, entry.value);
      }
      return succeed5(out);
    },
    encode: (formData) => {
      return succeed5(globalThis.Array.from(formData.entries()).map(([key, value]) => {
        if (typeof value === "string") {
          return [key, {
            _tag: "String",
            value
          }];
        } else {
          return [key, {
            _tag: "File",
            value
          }];
        }
      }));
    }
  }))
});
var URLSearchParams2 = /* @__PURE__ */ instanceOf(globalThis.URLSearchParams, {
  typeConstructor: {
    _tag: "URLSearchParams"
  },
  generation: {
    runtime: `Schema.URLSearchParams`,
    Type: `globalThis.URLSearchParams`
  },
  expected: "URLSearchParams",
  toCodecJson: () => link()(String4.annotate({
    expected: "a query string that will be decoded as URLSearchParams"
  }), transform2({
    decode: (e) => new globalThis.URLSearchParams(e),
    encode: (params) => params.toString()
  }))
});
var Base64String = /* @__PURE__ */ String4.annotate({
  expected: "a base64 encoded string that will be decoded as Uint8Array",
  format: "byte",
  contentEncoding: "base64"
});
var Uint8Array2 = /* @__PURE__ */ instanceOf(globalThis.Uint8Array, {
  typeConstructor: {
    _tag: "Uint8Array"
  },
  generation: {
    runtime: `Schema.Uint8Array`,
    Type: `globalThis.Uint8Array`
  },
  expected: "Uint8Array",
  toCodecJson: () => link()(Base64String, uint8ArrayFromBase64String),
  toArbitrary: () => (fc) => fc.uint8Array()
});
var immerable = /* @__PURE__ */ globalThis.Symbol.for("immer-draftable");
var payloadToken = {};
function makeClass(Inherited, identifier3, struct2, annotations, proto) {
  const getClassSchema = getClassSchemaFactory(struct2, identifier3, annotations);
  const ClassTypeId2 = getClassTypeId(identifier3);
  const out = class extends Inherited {
    constructor(...[input, options]) {
      const internalOptions = options;
      const payload = internalOptions?.["~payload"];
      const value = payload?.token === payloadToken ? payload.value : struct2.make(input ?? {}, options);
      super(value, {
        ...options,
        disableChecks: true,
        "~payload": {
          token: payloadToken,
          value
        }
      });
    }
    static [TypeId19] = TypeId19;
    get [ClassTypeId2]() {
      return ClassTypeId2;
    }
    static [immerable] = true;
    static identifier = identifier3;
    static fields = struct2.fields;
    static get ast() {
      return getClassSchema(this).ast;
    }
    static pipe() {
      return pipeArguments(this, arguments);
    }
    static rebuild(ast) {
      return getClassSchema(this).rebuild(ast);
    }
    static make(input, options) {
      return new this(input, options);
    }
    static makeOption(input, options) {
      return makeOption(getClassSchema(this))(input ?? {}, options);
    }
    static makeEffect(input, options) {
      return getClassSchema(this).makeEffect(input ?? {}, options);
    }
    static annotate(annotations2) {
      return this.rebuild(annotate(this.ast, annotations2));
    }
    static annotateKey(annotations2) {
      return this.rebuild(annotateKey(this.ast, annotations2));
    }
    static check(...checks) {
      return this.rebuild(appendChecks(this.ast, checks));
    }
    static extend(identifier4) {
      return (schema, annotations2) => {
        const extension = isStruct(schema) ? schema : Struct(schema);
        const fields = {
          ...struct2.fields,
          ...extension.fields
        };
        const ast = struct(fields, struct2.ast.checks, {
          identifier: identifier4
        });
        return makeClass(this, identifier4, makeStruct(appendChecks(ast, extension.ast.checks), fields), annotations2, proto);
      };
    }
    static mapFields(f, options) {
      return struct2.mapFields(f, options);
    }
  };
  if (proto !== void 0) {
    Object.assign(out.prototype, proto(identifier3));
  }
  return out;
}
function getClassTransformation(self2) {
  return new Transformation(transform((input) => new self2(input)), passthrough());
}
function getClassTypeId(identifier3) {
  return `~effect/Schema/Class/${identifier3}`;
}
function getClassSchemaFactory(from, identifier3, annotations) {
  let memo;
  return (self2) => {
    if (memo !== void 0) {
      return memo;
    }
    const transformation = getClassTransformation(self2);
    const to = make15(new Declaration([from.ast], () => (input, ast) => {
      return input instanceof self2 || hasProperty(input, getClassTypeId(identifier3)) ? succeed5(input) : fail5(new InvalidType(ast, some2(input)));
    }, {
      identifier: identifier3,
      [ClassTypeId]: ([from2]) => new Link(from2, transformation),
      toCodec: ([from2]) => new Link(from2.ast, transformation),
      toArbitrary: ([from2]) => () => ({
        arbitrary: from2.arbitrary.map((args2) => new self2(args2)),
        terminal: from2.terminal?.map((args2) => new self2(args2))
      }),
      toFormatter: ([from2]) => (t) => `${self2.identifier}(${from2(t)})`,
      "~sentinels": collectSentinels(from.ast),
      ...annotations
    }));
    return memo = decodeTo2(to, transformation)(from);
  };
}
function isStruct(schema) {
  return isSchema(schema);
}
var ErrorClass = (identifier3) => (schema, annotations) => {
  const struct2 = isStruct(schema) ? schema : Struct(schema);
  const self2 = makeClass(Error2, identifier3, struct2, annotations, (identifier4) => ({
    name: identifier4
  }));
  return self2;
};
var TaggedErrorClass = (identifier3) => {
  return (tagValue, schema, annotations) => {
    const struct2 = isStruct(schema) ? schema.mapFields((fields) => ({
      _tag: tag(tagValue),
      ...fields
    }), {
      unsafePreserveChecks: true
    }) : TaggedStruct(tagValue, schema);
    return ErrorClass(identifier3 ?? tagValue)(struct2, annotations);
  };
};
var Json2 = /* @__PURE__ */ make15(Json);

// src/schemas/runtime.ts
var RuntimeShellCommand = Struct({
  shell: String4
});
var RuntimeArgvCommand = Struct({
  cmd: String4,
  args: optional(ArraySchema(String4))
});
var RuntimeCommand = Union2([
  String4,
  RuntimeShellCommand,
  RuntimeArgvCommand
]);
var RuntimeExecOptions = Struct({
  cwd: optional(String4),
  timeoutMs: optional(Number5),
  env: optional(Record(String4, String4))
});
var RuntimeExecResult = Struct({
  stdout: String4,
  stderr: String4,
  exitCode: Number5,
  command: String4,
  timedOut: Boolean2
});
var RuntimeError = Struct({
  _tag: Literal2("RuntimeError"),
  operation: Literals(["read", "write", "exec", "spawn"]),
  message: String4,
  cause: optional(Unknown2)
});
var RuntimeProcessDescriptor = Struct({
  id: String4,
  command: String4
});
var decodeRuntimeCommand = decodeUnknownSync(RuntimeCommand);
var decodeRuntimeExecOptions = decodeUnknownSync(RuntimeExecOptions);
var decodeRuntimeExecResult = decodeUnknownSync(RuntimeExecResult);
var validateRuntimeCommand = (command) => decodeRuntimeCommand(command);
var validateRuntimeExecOptions = (options) => decodeRuntimeExecOptions(options);
var validateRuntimeExecResult = (result2) => decodeRuntimeExecResult(result2);

// src/services/MetatoolRuntime.ts
var MetatoolRuntime = class extends Service()(
  "@tmnl/metatool/MetatoolRuntime"
) {
};
var runtimeError = (operation, cause) => ({
  _tag: "RuntimeError",
  operation,
  message: cause instanceof Error ? cause.message : String(cause),
  cause
});
var renderCommand = (input) => {
  const command = validateRuntimeCommand(input);
  if (typeof command === "string") return command;
  if ("shell" in command) return command.shell;
  const args2 = command.args?.join(" ") ?? "";
  return args2.length > 0 ? `${command.cmd} ${args2}` : command.cmd;
};
function makeNodeRuntimeLayer(defaultCwd) {
  return effect(
    MetatoolRuntime,
    gen2(function* () {
      const fs = yield* FileSystem;
      const resolvePath = (path) => path.startsWith("/") ? path : join(defaultCwd, path);
      return MetatoolRuntime.of({
        read: (path) => fs.readFileString(resolvePath(path)).pipe(
          mapError3((cause) => runtimeError("read", cause))
        ),
        write: (path, content) => gen2(function* () {
          const abs = resolvePath(path);
          yield* fs.makeDirectory(dirname(abs), { recursive: true }).pipe(
            catchTag2("PlatformError", () => void_3),
            mapError3((cause) => runtimeError("write", cause))
          );
          yield* fs.writeFileString(abs, content).pipe(
            mapError3((cause) => runtimeError("write", cause))
          );
        }),
        exec: (input, rawOptions) => {
          const command = renderCommand(input);
          const options = rawOptions == null ? void 0 : validateRuntimeExecOptions(rawOptions);
          return tryPromise2({
            try: () => new Promise((resolve2) => {
              exec(
                command,
                {
                  cwd: options?.cwd ?? defaultCwd,
                  encoding: "utf-8",
                  timeout: options?.timeoutMs ?? 15e3,
                  env: options?.env ? { ...process.env, ...options.env } : process.env
                },
                (err, stdout, stderr) => {
                  const maybeError = err;
                  const rawCode = maybeError?.code;
                  const exitCode = typeof rawCode === "number" ? rawCode : rawCode == null ? 0 : 1;
                  resolve2(validateRuntimeExecResult({
                    stdout: stdout ?? "",
                    stderr: stderr ?? "",
                    exitCode,
                    command,
                    timedOut: maybeError?.killed === true
                  }));
                }
              );
            }),
            catch: (cause) => runtimeError("exec", cause)
          });
        },
        spawn: (input) => fail5(runtimeError(
          "spawn",
          new Error(`NodeRuntime does not expose managed spawn yet: ${renderCommand(input)}`)
        ))
      });
    })
  );
}

// node_modules/effect/dist/ManagedRuntime.js
var TypeId20 = "~effect/ManagedRuntime";
var make16 = (layer2, options) => {
  const memoMap = options?.memoMap ?? makeMemoMapUnsafe();
  const scope3 = makeUnsafe3("parallel");
  const layerScope = forkUnsafe2(scope3, "sequential");
  const defaultRunOptions = {
    onFiberStart: runIn(scope3)
  };
  const mergeRunOptions = (options2) => options2 ? {
    ...options2,
    onFiberStart: options2.onFiberStart ? (fiber2) => {
      defaultRunOptions.onFiberStart(fiber2);
      options2.onFiberStart(fiber2);
    } : defaultRunOptions.onFiberStart
  } : defaultRunOptions;
  let buildFiber;
  const contextEffect = withFiber2((fiber2) => {
    if (!buildFiber) {
      buildFiber = runFork2(tap2(buildWithMemoMap(layer2, memoMap, layerScope), (context3) => sync2(() => {
        self2.cachedContext = context3;
      })), {
        ...defaultRunOptions,
        scheduler: fiber2.currentScheduler
      });
    }
    return flatten2(await_(buildFiber));
  });
  const self2 = {
    [TypeId20]: TypeId20,
    memoMap,
    scope: scope3,
    contextEffect,
    cachedContext: void 0,
    context() {
      return self2.cachedContext === void 0 ? runPromise2(self2.contextEffect) : Promise.resolve(self2.cachedContext);
    },
    dispose() {
      return runPromise2(self2.disposeEffect);
    },
    disposeEffect: suspend2(() => {
      ;
      self2.contextEffect = die3("ManagedRuntime disposed");
      self2.cachedContext = void 0;
      return close(self2.scope, void_2);
    }),
    runFork(effect2, options2) {
      return self2.cachedContext === void 0 ? runFork2(provide4(self2, effect2), mergeRunOptions(options2)) : runForkWith2(self2.cachedContext)(effect2, mergeRunOptions(options2));
    },
    runCallback(effect2, options2) {
      return self2.cachedContext === void 0 ? runCallback2(provide4(self2, effect2), mergeRunOptions(options2)) : runCallbackWith2(self2.cachedContext)(effect2, mergeRunOptions(options2));
    },
    runSyncExit(effect2) {
      return self2.cachedContext === void 0 ? runSyncExit2(provide4(self2, effect2)) : runSyncExitWith2(self2.cachedContext)(effect2);
    },
    runSync(effect2) {
      return self2.cachedContext === void 0 ? runSync2(provide4(self2, effect2)) : runSyncWith2(self2.cachedContext)(effect2);
    },
    runPromiseExit(effect2, options2) {
      return self2.cachedContext === void 0 ? runPromiseExit2(provide4(self2, effect2), mergeRunOptions(options2)) : runPromiseExitWith2(self2.cachedContext)(effect2, mergeRunOptions(options2));
    },
    runPromise(effect2, options2) {
      return self2.cachedContext === void 0 ? runPromise2(provide4(self2, effect2), mergeRunOptions(options2)) : runPromiseWith2(self2.cachedContext)(effect2, mergeRunOptions(options2));
    }
  };
  return self2;
};
function provide4(managed, effect2) {
  return flatMap3(managed.contextEffect, (context3) => provideContext2(effect2, context3));
}

// node_modules/effect/dist/unstable/reactivity/Reactivity.js
var Reactivity = class extends (/* @__PURE__ */ Service()("effect/reactivity/Reactivity")) {
};

// node_modules/effect/dist/unstable/sql/Statement.js
var FragmentTypeId = "~effect/sql/Fragment";
var fragment = (segments) => ({
  [FragmentTypeId]: FragmentTypeId,
  segments
});
var CurrentTransformer = /* @__PURE__ */ Reference("effect/sql/CurrentTransformer", {
  defaultValue: constUndefined
});
var isFragment = (u) => hasProperty(u, FragmentTypeId);
var literal = (value, params) => ({
  _tag: "Literal",
  value,
  params
});
var identifier2 = (value) => ({
  _tag: "Identifier",
  value
});
var parameter = (value) => ({
  _tag: "Parameter",
  value
});
var arrayHelper = (value) => ({
  _tag: "ArrayHelper",
  value
});
var RecordInsertHelperProto = {
  _tag: "RecordInsertHelper",
  returning(sql) {
    const self2 = Object.create(Object.getPrototypeOf(this));
    Object.assign(self2, this, {
      returningIdentifier: sql
    });
    return self2;
  }
};
var recordInsertHelper = (value) => Object.assign(Object.create(RecordInsertHelperProto), {
  value,
  returningIdentifier: void 0
});
var RecordUpdateHelperProto = {
  ...RecordInsertHelperProto,
  _tag: "RecordUpdateHelper"
};
var recordUpdateHelper = (value, alias) => Object.assign(Object.create(RecordUpdateHelperProto), {
  value,
  alias,
  returningIdentifier: void 0
});
var RecordUpdateHelperSingleProto = {
  ...RecordInsertHelperProto,
  _tag: "RecordUpdateHelperSingle"
};
var recordUpdateHelperSingle = (value, omit2) => Object.assign(Object.create(RecordUpdateHelperSingleProto), {
  value,
  omit: omit2,
  returningIdentifier: void 0
});
var make17 = (acquirer, compiler, spanAttributes, transformRows) => {
  const cache = transformRows === void 0 ? constructorCache.noTransforms : constructorCache.transforms;
  if (cache.has(acquirer)) {
    return cache.get(acquirer);
  }
  const self2 = Object.assign(function sql(strings, ...args2) {
    if (typeof strings === "string") {
      return identifier2(strings);
    } else if (Array.isArray(strings) && "raw" in strings) {
      return statement(acquirer, compiler, strings, args2, spanAttributes, transformRows);
    }
    throw "absurd";
  }, {
    unsafe(sql, params) {
      return makeUnsafe4([literal(sql, params)], acquirer, compiler, spanAttributes, transformRows);
    },
    literal(sql) {
      return fragment([literal(sql)]);
    },
    in: in_,
    insert(value) {
      return recordInsertHelper(Array.isArray(value) ? value : [value]);
    },
    update(value, omit2) {
      return recordUpdateHelperSingle(value, omit2 ?? []);
    },
    updateValues(value, alias) {
      return recordUpdateHelper(value, alias);
    },
    and,
    or,
    csv,
    join: join2,
    onDialect(options) {
      return options[compiler.dialect]();
    },
    onDialectOrElse(options) {
      return options[compiler.dialect] !== void 0 ? options[compiler.dialect]() : options.orElse();
    }
  });
  cache.set(acquirer, self2);
  return self2;
};
var constructorCache = {
  transforms: /* @__PURE__ */ new WeakMap(),
  noTransforms: /* @__PURE__ */ new WeakMap()
};
var statement = (acquirer, compiler, strings, args2, spanAttributes, transformRows) => {
  const segments = strings[0].length > 0 ? [literal(strings[0])] : [];
  for (let i = 0; i < args2.length; i++) {
    const arg = args2[i];
    if (isFragment(arg)) {
      segments.push(...arg.segments);
    } else if (isSegment(arg)) {
      segments.push(arg);
    } else {
      segments.push(parameter(arg));
    }
    if (strings[i + 1].length > 0) {
      segments.push(literal(strings[i + 1]));
    }
  }
  return makeUnsafe4(segments, acquirer, compiler, spanAttributes, transformRows);
};
function join2(lit, addParens = true, fallback = "") {
  const literalStatement = literal(lit);
  const fallbackFragment = fragment([literal(fallback)]);
  return (clauses) => {
    if (clauses.length === 0) {
      return fallbackFragment;
    } else if (clauses.length === 1) {
      return fragment(convertLiteralOrFragment(clauses[0]));
    }
    const segments = [];
    if (addParens) {
      segments.push(literal("("));
    }
    segments.push.apply(segments, convertLiteralOrFragment(clauses[0]));
    for (let i = 1; i < clauses.length; i++) {
      segments.push(literalStatement);
      segments.push.apply(segments, convertLiteralOrFragment(clauses[i]));
    }
    if (addParens) {
      segments.push(literal(")"));
    }
    return fragment(segments);
  };
}
var and = /* @__PURE__ */ join2(" AND ", true, "1=1");
var or = /* @__PURE__ */ join2(" OR ", true, "1=1");
var csv = function(...args2) {
  if (args2[args2.length - 1].length === 0) {
    return emptyFragment;
  }
  if (args2.length === 1) {
    return csvRaw(args2[0]);
  }
  return fragment([literal(`${args2[0]} `), ...csvRaw(args2[1]).segments]);
};
var csvRaw = /* @__PURE__ */ join2(",", false);
var emptyFragment = /* @__PURE__ */ fragment([/* @__PURE__ */ literal("")]);
var makeCompiler = (options) => {
  const self2 = Object.create(CompilerProto);
  self2.options = options;
  self2.dialect = options.dialect;
  self2.disableTransforms = false;
  return self2;
};
var statementCacheSymbol = /* @__PURE__ */ Symbol.for("effect/unstable/sql/Statement/statementCache");
var statementCacheNoTransformSymbol = /* @__PURE__ */ Symbol.for("effect/unstable/sql/Statement/statementCacheNoTransform");
var CompilerProto = {
  compile(statement2, withoutTransform = false, placeholderOverride) {
    const opts = this.options;
    withoutTransform = withoutTransform || this.disableTransforms;
    const cacheSymbol = withoutTransform ? statementCacheNoTransformSymbol : statementCacheSymbol;
    if (cacheSymbol in statement2) {
      return statement2[cacheSymbol];
    }
    const segments = statement2.segments;
    const len = segments.length;
    let sql = "";
    const binds = [];
    let placeholderCount = 0;
    const placeholder = placeholderOverride ?? ((u) => opts.placeholder(++placeholderCount, u));
    const placeholderNoIncrement = (u) => opts.placeholder(placeholderCount, u);
    const placeholders = makePlaceholdersArray(placeholder);
    for (let i = 0; i < len; i++) {
      const segment = segments[i];
      switch (segment._tag) {
        case "Literal": {
          sql += segment.value;
          if (segment.params) {
            binds.push.apply(binds, segment.params);
          }
          break;
        }
        case "Identifier": {
          sql += opts.onIdentifier(segment.value, withoutTransform);
          break;
        }
        case "Parameter": {
          sql += placeholder(segment.value);
          binds.push(segment.value);
          break;
        }
        case "ArrayHelper": {
          sql += `(${placeholders(segment.value)})`;
          binds.push.apply(binds, segment.value);
          break;
        }
        case "RecordInsertHelper": {
          const keys = Object.keys(segment.value[0]);
          if (opts.onInsert) {
            const values = new Array(segment.value.length);
            let placeholders2 = "";
            for (let i2 = 0; i2 < segment.value.length; i2++) {
              const row = new Array(keys.length);
              values[i2] = row;
              placeholders2 += i2 === 0 ? "(" : ",(";
              for (let j = 0; j < keys.length; j++) {
                const key = keys[j];
                const value = segment.value[i2][key];
                const primitive = extractPrimitive(value, opts.onCustom, placeholderNoIncrement, withoutTransform);
                row[j] = primitive;
                placeholders2 += j === 0 ? placeholder(value) : `,${placeholder(value)}`;
              }
              placeholders2 += ")";
            }
            const [s, b] = opts.onInsert(keys.map((_) => opts.onIdentifier(_, withoutTransform)), placeholders2, values, typeof segment.returningIdentifier === "string" ? [segment.returningIdentifier, []] : segment.returningIdentifier ? this.compile(segment.returningIdentifier, withoutTransform, placeholder) : void 0);
            sql += s;
            binds.push.apply(binds, b);
          } else {
            let placeholders2 = "";
            for (let i2 = 0; i2 < segment.value.length; i2++) {
              placeholders2 += i2 === 0 ? "(" : ",(";
              for (let j = 0; j < keys.length; j++) {
                const value = segment.value[i2][keys[j]];
                const primitive = extractPrimitive(value, opts.onCustom, placeholderNoIncrement, withoutTransform);
                binds.push(primitive);
                placeholders2 += j === 0 ? placeholder(value) : `,${placeholder(value)}`;
              }
              placeholders2 += ")";
            }
            sql += `${generateColumns(keys, opts.onIdentifier, withoutTransform)} VALUES ${placeholders2}`;
            if (typeof segment.returningIdentifier === "string") {
              sql += ` RETURNING ${segment.returningIdentifier}`;
            } else if (segment.returningIdentifier) {
              sql += " RETURNING ";
              const [s, b] = this.compile(segment.returningIdentifier, withoutTransform, placeholder);
              sql += s;
              binds.push.apply(binds, b);
            }
          }
          break;
        }
        case "RecordUpdateHelperSingle": {
          let keys = Object.keys(segment.value);
          if (segment.omit.length > 0) {
            keys = keys.filter((key) => !segment.omit.includes(key));
          }
          if (opts.onRecordUpdateSingle) {
            const [s, b] = opts.onRecordUpdateSingle(keys.map((_) => opts.onIdentifier(_, withoutTransform)), keys.map((key) => extractPrimitive(segment.value[key], opts.onCustom, placeholderNoIncrement, withoutTransform)), typeof segment.returningIdentifier === "string" ? [segment.returningIdentifier, []] : segment.returningIdentifier ? this.compile(segment.returningIdentifier, withoutTransform, placeholder) : void 0);
            sql += s;
            binds.push.apply(binds, b);
          } else {
            for (let i2 = 0, len2 = keys.length; i2 < len2; i2++) {
              const column = opts.onIdentifier(keys[i2], withoutTransform);
              if (i2 === 0) {
                sql += `${column} = ${placeholder(segment.value[keys[i2]])}`;
              } else {
                sql += `, ${column} = ${placeholder(segment.value[keys[i2]])}`;
              }
              binds.push(extractPrimitive(segment.value[keys[i2]], opts.onCustom, placeholderNoIncrement, withoutTransform));
            }
            if (typeof segment.returningIdentifier === "string") {
              if (this.dialect === "mssql") {
                sql += ` OUTPUT ${segment.returningIdentifier === "*" ? "INSERTED.*" : segment.returningIdentifier}`;
              } else {
                sql += ` RETURNING ${segment.returningIdentifier}`;
              }
            } else if (segment.returningIdentifier) {
              sql += this.dialect === "mssql" ? " OUTPUT " : " RETURNING ";
              const [s, b] = this.compile(segment.returningIdentifier, withoutTransform, placeholder);
              sql += s;
              binds.push.apply(binds, b);
            }
          }
          break;
        }
        case "RecordUpdateHelper": {
          const keys = Object.keys(segment.value[0]);
          const values = new Array(segment.value.length);
          let placeholders2 = "";
          for (let i2 = 0; i2 < segment.value.length; i2++) {
            const row = new Array(keys.length);
            values[i2] = row;
            placeholders2 += i2 === 0 ? "(" : ",(";
            for (let j = 0; j < keys.length; j++) {
              const key = keys[j];
              const value = segment.value[i2][key];
              row[j] = extractPrimitive(value, opts.onCustom, placeholderNoIncrement, withoutTransform);
              placeholders2 += j === 0 ? placeholder(value) : `,${placeholder(value)}`;
            }
            placeholders2 += ")";
          }
          const [s, b] = opts.onRecordUpdate(placeholders2, segment.alias, generateColumns(keys, opts.onIdentifier, withoutTransform), values, typeof segment.returningIdentifier === "string" ? [segment.returningIdentifier, []] : segment.returningIdentifier ? this.compile(segment.returningIdentifier, withoutTransform, placeholder) : void 0);
          sql += s;
          binds.push.apply(binds, b);
          break;
        }
        case "Custom": {
          const [s, b] = opts.onCustom(segment, placeholder, withoutTransform);
          sql += s;
          binds.push.apply(binds, b);
          break;
        }
      }
    }
    const result2 = [sql, binds];
    if (placeholderOverride !== void 0) {
      return result2;
    }
    return statement2[cacheSymbol] = result2;
  },
  get withoutTransform() {
    const self2 = Object.create(CompilerProto);
    Object.assign(self2, this, {
      disableTransforms: true
    });
    return self2;
  }
};
var makeCompilerSqlite = (transform3) => makeCompiler({
  dialect: "sqlite",
  placeholder(_) {
    return "?";
  },
  onIdentifier: transform3 ? function(value, withoutTransform) {
    return withoutTransform ? escapeSqlite(value) : escapeSqlite(transform3(value));
  } : escapeSqlite,
  onRecordUpdate() {
    return ["", []];
  },
  onCustom() {
    return ["", []];
  }
});
function defaultEscape(c) {
  const re = new RegExp(c, "g");
  const double = c + c;
  const dot = c + "." + c;
  return function(str) {
    return c + str.replace(re, double).replace(/\./g, dot) + c;
  };
}
var defaultTransforms = (transformer, nested = true) => {
  const transformValue = (value) => {
    if (Array.isArray(value)) {
      if (value.length === 0 || value[0].constructor !== Object) {
        return value;
      }
      return array2(value);
    } else if (value?.constructor === Object) {
      return transformObject(value);
    }
    return value;
  };
  const transformObject = (obj) => {
    const newObj = {};
    for (const key in obj) {
      newObj[transformer(key)] = transformValue(obj[key]);
    }
    return newObj;
  };
  const transformArrayNested = (rows) => {
    const newRows = new Array(rows.length);
    for (let i = 0, len = rows.length; i < len; i++) {
      const row = rows[i];
      if (Array.isArray(row)) {
        newRows[i] = transformArrayNested(row);
      } else {
        const obj = {};
        for (const key in row) {
          obj[transformer(key)] = transformValue(row[key]);
        }
        newRows[i] = obj;
      }
    }
    return newRows;
  };
  const transformArray = (rows) => {
    const newRows = new Array(rows.length);
    for (let i = 0, len = rows.length; i < len; i++) {
      const row = rows[i];
      if (Array.isArray(row)) {
        newRows[i] = transformArray(row);
      } else {
        const obj = {};
        for (const key in row) {
          obj[transformer(key)] = row[key];
        }
        newRows[i] = obj;
      }
    }
    return newRows;
  };
  const array2 = nested ? transformArrayNested : transformArray;
  return {
    value: transformValue,
    object: transformObject,
    array: array2
  };
};
var ATTR_DB_OPERATION_NAME = "db.operation.name";
var ATTR_DB_QUERY_TEXT = "db.query.text";
var makeUnsafe4 = (segments, acquirer, compiler, spanAttributes, transformRows) => {
  const self2 = Object.create(StatementProto);
  self2.segments = segments;
  self2.acquirer = acquirer;
  self2.compiler = compiler;
  self2.spanAttributes = spanAttributes;
  self2.transformRows = transformRows;
  return self2;
};
var StatementProto = {
  .../* @__PURE__ */ Prototype2({
    label: "Statement",
    evaluate(fiber2) {
      const span = makeSpanUnsafe(fiber2, "sql.execute", {
        kind: "client"
      });
      const clock = fiber2.getRef(Clock);
      const timingEnabled = fiber2.getRef(TracerTimingEnabled2);
      return onExit2(this.withConnectionSpan("execute", (connection, sql, params) => connection.execute(sql, params, this.transformRows), false, span), (exit3) => endSpan(span, exit3, clock, timingEnabled));
    }
  }),
  [FragmentTypeId]: FragmentTypeId,
  withConnection(operation, f, withoutTransform = false) {
    return useSpan2("sql.execute", {
      kind: "client"
    }, (span) => this.withConnectionSpan(operation, f, withoutTransform, span));
  },
  withConnectionSpan(operation, f, withoutTransform, span) {
    return withStatement(this, span, (statement2) => {
      const [sql, params] = statement2.compile(withoutTransform);
      for (const [key, value] of this.spanAttributes) {
        span.attribute(key, value);
      }
      span.attribute(ATTR_DB_OPERATION_NAME, operation);
      span.attribute(ATTR_DB_QUERY_TEXT, sql);
      return scoped2(flatMap3(this.acquirer, (_) => f(_, sql, params)));
    });
  },
  get withoutTransform() {
    return this.withConnection("executeWithoutTransform", (connection, sql, params) => connection.execute(sql, params, void 0), true);
  },
  get raw() {
    return this.withConnection("executeRaw", (connection, sql, params) => connection.executeRaw(sql, params), true);
  },
  get stream() {
    const self2 = this;
    return unwrap3(flatMap3(makeSpanScoped2("sql.execute", {
      kind: "client"
    }), (span) => withStatement(self2, span, (statement2) => {
      const [sql, params] = statement2.compile();
      for (const [key, value] of self2.spanAttributes) {
        span.attribute(key, value);
      }
      span.attribute(ATTR_DB_OPERATION_NAME, "executeStream");
      span.attribute(ATTR_DB_QUERY_TEXT, sql);
      return map5(self2.acquirer, (_) => _.executeStream(sql, params, self2.transformRows));
    })));
  },
  get values() {
    return this.withConnection("executeValues", (connection, sql, params) => connection.executeValues(sql, params));
  },
  get valuesUnprepared() {
    return this.withConnection("executeValuesUnprepared", (connection, sql, params) => connection.executeValuesUnprepared(sql, params));
  },
  get unprepared() {
    const self2 = this;
    return self2.withConnection("executeUnprepared", (connection, sql, params) => connection.executeUnprepared(sql, params, self2.transformRows));
  },
  compile(withoutTransform) {
    return this.compiler.compile(this, withoutTransform ?? false);
  },
  toJSON() {
    const [sql, params] = this.compile();
    return {
      _id: "Statement",
      segments: this.segments,
      sql,
      params
    };
  }
};
var withStatement = (self2, span, f) => withFiber2((fiber2) => {
  const transform3 = fiber2.getRef(CurrentTransformer);
  if (transform3 === void 0) {
    return f(self2);
  }
  return flatMap3(transform3(self2, make17(self2.acquirer, self2.compiler, self2.spanAttributes, self2.transformRows), fiber2, span), f);
});
var isSegment = (u) => {
  if (!hasProperty(u, "_tag")) {
    return false;
  }
  switch (u._tag) {
    case "Literal":
    case "Parameter":
    case "ArrayHelper":
    case "RecordInsertHelper":
    case "RecordUpdateHelper":
    case "RecordUpdateHelperSingle":
    case "Identifier":
    case "Custom":
      return true;
    default:
      return false;
  }
};
function convertLiteralOrFragment(clause) {
  if (typeof clause === "string") {
    return [literal(clause)];
  }
  return clause.segments;
}
var makePlaceholdersArray = (evaluate2) => (values) => {
  if (values.length === 0) {
    return "";
  }
  let result2 = evaluate2(values[0]);
  for (let i = 1; i < values.length; i++) {
    result2 += `,${evaluate2(values[i])}`;
  }
  return result2;
};
var generateColumns = (keys, escape, withoutTransform) => {
  if (keys.length === 0) {
    return "()";
  }
  let str = `(${escape(keys[0], withoutTransform)}`;
  for (let i = 1; i < keys.length; i++) {
    str += `,${escape(keys[i], withoutTransform)}`;
  }
  return str + ")";
};
var extractPrimitive = (value, onCustom, placeholder, withoutTransform) => {
  if (value === void 0) {
    return null;
  } else if (isFragment(value)) {
    const head = value.segments[0];
    if (head._tag === "Custom") {
      const compiled = onCustom(head, placeholder, withoutTransform);
      return compiled[1][0] ?? null;
    } else if (head._tag === "Parameter") {
      return head.value;
    }
    return null;
  }
  return value;
};
var escapeSqlite = /* @__PURE__ */ defaultEscape('"');
function in_() {
  if (arguments.length === 1) {
    return arrayHelper(arguments[0]);
  }
  const column = arguments[0];
  const values = arguments[1];
  return values.length === 0 ? neverFragment : fragment([identifier2(column), literal(" IN "), arrayHelper(values)]);
}
var neverFragment = /* @__PURE__ */ fragment([/* @__PURE__ */ literal("1=0")]);

// node_modules/effect/dist/unstable/sql/SqlClient.js
var TypeId21 = "~effect/sql/SqlClient";
var SqlClient = /* @__PURE__ */ Service("effect/sql/SqlClient");
var clientIdCounter = 0;
var make18 = /* @__PURE__ */ fnUntraced2(function* (options) {
  const transactionService = options.transactionService ?? TransactionConnection(clientIdCounter++);
  const getConnection = flatMap3(serviceOption2(transactionService), match({
    onNone: () => options.acquirer,
    onSome: ([conn]) => succeed5(conn)
  }));
  const beginTransaction = options.beginTransaction ?? "BEGIN";
  const commit = options.commit ?? "COMMIT";
  const savepoint = options.savepoint ?? ((name) => `SAVEPOINT ${name}`);
  const rollback = options.rollback ?? "ROLLBACK";
  const rollbackSavepoint = options.rollbackSavepoint ?? ((name) => `ROLLBACK TO SAVEPOINT ${name}`);
  const transactionAcquirer = options.transactionAcquirer ?? options.acquirer;
  const withTransaction = makeWithTransaction({
    transactionService,
    spanAttributes: options.spanAttributes,
    acquireConnection: flatMap3(make5(), (scope3) => map5(provide(transactionAcquirer, scope3), (conn) => [scope3, conn])),
    begin: (conn) => conn.executeUnprepared(beginTransaction, [], void 0),
    savepoint: (conn, id) => conn.executeUnprepared(savepoint(`effect_sql_${id}`), [], void 0),
    commit: (conn) => conn.executeUnprepared(commit, [], void 0),
    rollback: (conn) => conn.executeUnprepared(rollback, [], void 0),
    rollbackSavepoint: (conn, id) => conn.executeUnprepared(rollbackSavepoint(`effect_sql_${id}`), [], void 0)
  });
  const reactivity = yield* Reactivity;
  const client = Object.assign(make17(getConnection, options.compiler, options.spanAttributes, options.transformRows), {
    [TypeId21]: TypeId21,
    safe: void 0,
    withTransaction,
    transactionService,
    reserve: transactionAcquirer,
    withoutTransforms() {
      if (options.transformRows === void 0) {
        return this;
      }
      const statement2 = make17(getConnection, options.compiler.withoutTransform, options.spanAttributes, void 0);
      const client2 = Object.assign(statement2, {
        ...this,
        ...statement2
      });
      client2.safe = client2;
      client2.withoutTransforms = () => client2;
      return client2;
    },
    reactive: options.reactiveQueue ? (keys, effect2) => options.reactiveQueue(keys, effect2).pipe(map5(fromQueue), unwrap3) : reactivity.stream,
    reactiveMailbox: options.reactiveQueue ?? reactivity.query
  });
  client.safe = client;
  return client;
});
var makeWithTransaction = (options) => (effect2) => {
  return uninterruptibleMask2((restore) => useSpan2("sql.transaction", {
    kind: "client"
  }, (span) => withFiber2((fiber2) => {
    for (const [key, value] of options.spanAttributes) {
      span.attribute(key, value);
    }
    const services = fiber2.context;
    const clock = fiber2.getRef(Clock);
    const connOption = getOption(services, options.transactionService);
    const conn = connOption._tag === "Some" ? succeed5([void 0, connOption.value[0]]) : options.acquireConnection;
    const id = connOption._tag === "Some" ? connOption.value[1] + 1 : 0;
    return flatMap3(conn, ([scope3, conn2]) => (id === 0 ? options.begin(conn2) : options.savepoint(conn2, id)).pipe(flatMap3(() => provideContext2(restore(effect2), mutate(services, (services2) => services2.pipe(add(options.transactionService, [conn2, id]), add(ParentSpan, span))))), exit2, flatMap3((exit3) => {
      let effect3;
      if (isSuccess3(exit3)) {
        if (id === 0) {
          span.event("db.transaction.commit", clock.currentTimeNanosUnsafe());
          effect3 = orDie2(options.commit(conn2));
        } else {
          span.event("db.transaction.savepoint", clock.currentTimeNanosUnsafe());
          effect3 = void_3;
        }
      } else {
        span.event("db.transaction.rollback", clock.currentTimeNanosUnsafe());
        effect3 = orDie2(id > 0 ? options.rollbackSavepoint(conn2, id) : options.rollback(conn2));
      }
      const withScope = scope3 !== void 0 ? ensuring2(effect3, close(scope3, exit3)) : effect3;
      return flatMap3(withScope, () => exit3);
    })));
  })));
};
var TransactionConnection = (clientId) => Service(`effect/sql/SqlClient/TransactionConnection/${clientId}`);

// src/store/schemas.ts
var NAMESPACE_PATTERN = /^[a-z][a-z0-9-]*(\.[a-z][a-z0-9-]*){0,2}$/;
var SYSTEM_PREFIX = "_system";
var Namespace = String4.pipe(
  check(makeFilter2(
    (s) => s.startsWith(SYSTEM_PREFIX) || NAMESPACE_PATTERN.test(s) ? void 0 : `Invalid namespace "${s}". Must be 1-3 dot-separated kebab segments (e.g. "domain.category.sub")`
  )),
  brand2("Namespace")
);
var namespaceMatchesGlob = (ns, glob) => {
  if (glob === "*") return true;
  if (!glob.includes("*")) return ns === glob;
  const prefix = glob.replace(/\.\*$/, "").replace(/\*$/, "");
  return ns === prefix || ns.startsWith(prefix) || ns.startsWith(prefix + ".");
};
var KEY_PATTERN = /^[a-z][a-z0-9-]*(--\d{8}T\d{6})?$/;
var StoreKey = String4.pipe(
  check(makeFilter2(
    (s) => KEY_PATTERN.test(s) ? void 0 : `Invalid key "${s}". Must be kebab-case, optionally --YYYYMMDDTHHMMSS`
  )),
  brand2("StoreKey")
);
var temporalSuffix = () => {
  const now = /* @__PURE__ */ new Date();
  const y = now.getFullYear();
  const m = String(now.getMonth() + 1).padStart(2, "0");
  const d = String(now.getDate()).padStart(2, "0");
  const h = String(now.getHours()).padStart(2, "0");
  const mi = String(now.getMinutes()).padStart(2, "0");
  const s = String(now.getSeconds()).padStart(2, "0");
  return `--${y}${m}${d}T${h}${mi}${s}`;
};
var ObjectMetaCore = Struct({
  summary: NonEmptyString,
  source: optional(String4),
  intent: optional(String4),
  schema: optional(String4)
});
var validateMeta = (meta) => {
  if (meta == null || typeof meta !== "object") {
    throw new Error("_meta must be an object with at least { summary: string }");
  }
  const m = meta;
  if (typeof m.summary !== "string" || m.summary.trim().length === 0) {
    throw new Error("_meta.summary is required and must be a non-empty string");
  }
  return m;
};
var CollectionConfig = Struct({
  description: String4,
  icon: optional(String4),
  retention: optional(String4)
});
var DomainConfig = Struct({
  description: String4,
  collections: Record(String4, CollectionConfig),
  meta: Struct({
    required: ArraySchema(String4),
    recommended: optional(ArraySchema(String4))
  })
});
var decodeNamespace = decodeUnknownSync(Namespace);
var decodeStoreKey = decodeUnknownSync(StoreKey);
var decodeDomainConfig = decodeUnknownSync(DomainConfig);
var validateNamespace = (ns) => decodeNamespace(ns);
var validateKey = (key) => decodeStoreKey(key);
var validateDomainConfig = (config) => decodeDomainConfig(config);

// node_modules/effect/dist/unstable/sql/Migrator.js
var MigrationError = class extends (/* @__PURE__ */ TaggedError2("MigrationError")) {
};
var make19 = ({
  dumpSchema = () => void_3
}) => ({
  loader,
  schemaDirectory,
  table = "effect_sql_migrations"
}) => gen2(function* () {
  const sql = yield* SqlClient;
  const ensureMigrationsTable = sql.onDialectOrElse({
    mssql: () => sql`IF OBJECT_ID(N'${sql.literal(table)}', N'U') IS NULL
  CREATE TABLE ${sql(table)} (
    migration_id INT NOT NULL PRIMARY KEY,
    name VARCHAR(255) NOT NULL,
    created_at DATETIME NOT NULL DEFAULT GETDATE()
  )`,
    mysql: () => sql`CREATE TABLE IF NOT EXISTS ${sql(table)} (
  migration_id INTEGER UNSIGNED NOT NULL,
  created_at TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP,
  name VARCHAR(255) NOT NULL,
  PRIMARY KEY (migration_id)
)`,
    pg: () => catch_2(sql`select ${table}::regclass`, () => sql`CREATE TABLE ${sql(table)} (
  migration_id integer primary key,
  created_at timestamp with time zone not null default now(),
  name text not null
)`),
    orElse: () => sql`CREATE TABLE IF NOT EXISTS ${sql(table)} (
  migration_id integer PRIMARY KEY NOT NULL,
  created_at datetime NOT NULL DEFAULT current_timestamp,
  name VARCHAR(255) NOT NULL
)`
  });
  const insertMigrations = (rows) => sql`INSERT INTO ${sql(table)} ${sql.insert(rows.map(([migration_id, name]) => ({
    migration_id,
    name
  })))}`.withoutTransform;
  const latestMigration = map5(sql`SELECT migration_id, name, created_at FROM ${sql(table)} ORDER BY migration_id DESC`.withoutTransform, (_) => map(fromNullishOr(_[0]), ({
    created_at,
    migration_id,
    name
  }) => ({
    id: migration_id,
    name,
    createdAt: created_at
  })));
  const loadMigration = ([id, name, load]) => catchDefect2(load, (_) => fail5(new MigrationError({
    kind: "ImportError",
    message: `Could not import migration "${id}_${name}"

${_}`
  }))).pipe(flatMap3((_) => isEffect2(_) ? succeed5(_) : _.default ? succeed5(_.default?.default ?? _.default) : fail5(new MigrationError({
    kind: "ImportError",
    message: `Default export not found for migration "${id}_${name}"`
  }))), filterOrFail2(isEffect2, () => new MigrationError({
    kind: "ImportError",
    message: `Default export was not an Effect for migration "${id}_${name}"`
  })));
  const runMigration = (id, name, effect2) => catch_2(effect2, (error) => die3(new MigrationError({
    cause: error,
    kind: "Failed",
    message: `Migration "${id}_${name}" failed`
  })));
  const run2 = gen2(function* () {
    yield* sql.onDialectOrElse({
      pg: () => sql`LOCK TABLE ${sql(table)} IN ACCESS EXCLUSIVE MODE`,
      orElse: () => void_3
    });
    const [latestMigrationId, current] = yield* all2([map5(latestMigration, match({
      onNone: () => 0,
      onSome: (_) => _.id
    })), loader]);
    if (new Set(current.map(([id]) => id)).size !== current.length) {
      return yield* new MigrationError({
        kind: "Duplicates",
        message: "Found duplicate migration id's"
      });
    }
    const required = [];
    for (const resolved of current) {
      const [currentId, currentName] = resolved;
      if (currentId <= latestMigrationId) {
        continue;
      }
      required.push([currentId, currentName, yield* loadMigration(resolved)]);
    }
    if (required.length > 0) {
      yield* pipe(insertMigrations(required.map(([id, name]) => [id, name])), mapError3((error) => isConstraintConflict(error) ? new MigrationError({
        kind: "Locked",
        message: "Migrations already running"
      }) : error));
    }
    yield* forEach2(required, ([id, name, effect2]) => logDebug(`Running migration`).pipe(flatMap3(() => runMigration(id, name, effect2)), annotateLogs("migration_id", String(id)), annotateLogs("migration_name", name), withSpan2(`Migrator ${id}_${name}`)), {
      discard: true
    });
    yield* pipe(latestMigration, flatMap3(match({
      onNone: () => logDebug(`Migrations complete`),
      onSome: (_) => logDebug(`Migrations complete`).pipe(annotateLogs("latest_migration_id", _.id.toString()), annotateLogs("latest_migration_name", _.name))
    })));
    return required.map(([id, name]) => [id, name]);
  });
  yield* ensureMigrationsTable;
  const completed = yield* pipe(sql.withTransaction(run2), catchTag2("MigrationError", (_) => _.kind === "Locked" ? as2(logDebug(_.message), []) : fail5(_)));
  if (schemaDirectory && completed.length > 0) {
    yield* dumpSchema(`${schemaDirectory}/_schema.sql`, table).pipe(catchCause2((cause) => logInfo("Could not dump schema", cause)));
  }
  return completed;
});
var migrationOrder = /* @__PURE__ */ make(([a], [b]) => Number2(a, b));
var isConstraintConflict = (error) => error.reason._tag === "ConstraintError" || error.reason._tag === "UniqueViolation";
var fromRecord = (migrations2) => pipe(Object.keys(migrations2), flatMapNullishOr((_) => _.match(/^(\d+)_(.+)$/)), map2(([key, id, name]) => [Number(id), name, succeed5(migrations2[key])]), sort(migrationOrder), succeed5);

// src/store/migrations.ts
var MigrationsComplete = class extends Service()(
  "@tmnl/rlm/MigrationsComplete"
) {
};
var migrations = {
  "0001_objects_table": gen2(function* () {
    const sql = yield* SqlClient;
    yield* sql.unsafe(`
      CREATE TABLE IF NOT EXISTS objects (
        collection TEXT NOT NULL,
        key TEXT NOT NULL,
        data TEXT NOT NULL,
        tags TEXT DEFAULT '[]',
        summary TEXT,
        intent TEXT,
        source TEXT,
        created_at TEXT DEFAULT (datetime('now')),
        updated_at TEXT DEFAULT (datetime('now')),
        PRIMARY KEY (collection, key)
      )
    `);
  }),
  "0002_objects_indexes": gen2(function* () {
    const sql = yield* SqlClient;
    yield* sql.unsafe(
      `CREATE INDEX IF NOT EXISTS idx_objects_collection ON objects(collection)`
    );
    yield* sql.unsafe(
      `CREATE INDEX IF NOT EXISTS idx_objects_summary ON objects(summary)`
    );
  }),
  "0003_fts5_virtual_table": gen2(function* () {
    const sql = yield* SqlClient;
    yield* sql.unsafe(`
      CREATE VIRTUAL TABLE IF NOT EXISTS objects_fts USING fts5(
        summary, intent, source,
        content=objects,
        content_rowid=rowid
      )
    `);
  }),
  "0004_fts5_triggers": gen2(function* () {
    const sql = yield* SqlClient;
    yield* sql.unsafe(`
      CREATE TRIGGER IF NOT EXISTS objects_fts_ai AFTER INSERT ON objects BEGIN
        INSERT INTO objects_fts(rowid, summary, intent, source)
        VALUES (new.rowid, new.summary, new.intent, new.source);
      END
    `);
    yield* sql.unsafe(`
      CREATE TRIGGER IF NOT EXISTS objects_fts_ad AFTER DELETE ON objects BEGIN
        INSERT INTO objects_fts(objects_fts, rowid, summary, intent, source)
        VALUES ('delete', old.rowid, old.summary, old.intent, old.source);
      END
    `);
    yield* sql.unsafe(`
      CREATE TRIGGER IF NOT EXISTS objects_fts_au AFTER UPDATE ON objects BEGIN
        INSERT INTO objects_fts(objects_fts, rowid, summary, intent, source)
        VALUES ('delete', old.rowid, old.summary, old.intent, old.source);
        INSERT INTO objects_fts(rowid, summary, intent, source)
        VALUES (new.rowid, new.summary, new.intent, new.source);
      END
    `);
  })
};
var runMigrations = make19({})({
  loader: fromRecord(migrations),
  table: "rlm_migrations"
});
var MigrationLayer = effect(
  MigrationsComplete,
  runMigrations.pipe(
    map5(() => MigrationsComplete.of({ _tag: "MigrationsComplete" }))
  )
);

// src/store/service.ts
var RlmStore = class extends Service()(
  "@tmnl/rlm/Store"
) {
};
var RlmStoreLive = effect(
  RlmStore,
  gen2(function* () {
    const sql = yield* SqlClient;
    yield* MigrationsComplete;
    return RlmStore.of({
      put: (ns, key, data, opts) => gen2(function* () {
        const validNs = validateNamespace(ns);
        const validKey = validateKey(key);
        const meta = data._meta;
        if (meta) {
          validateMeta(meta);
        }
        const summary = meta?.summary;
        const intent = meta?.intent;
        const source = meta?.source;
        const tags = JSON.stringify(opts?.tags ?? []);
        const jsonData = JSON.stringify(data);
        yield* sql`
            INSERT INTO objects (collection, key, data, tags, summary, intent, source, updated_at)
            VALUES (${validNs}, ${validKey}, ${jsonData}, ${tags}, ${summary ?? null}, ${intent ?? null}, ${source ?? null}, datetime('now'))
            ON CONFLICT(collection, key) DO UPDATE SET
              data = excluded.data,
              tags = excluded.tags,
              summary = excluded.summary,
              intent = excluded.intent,
              source = excluded.source,
              updated_at = datetime('now')
          `;
        return { ns: validNs, key: validKey };
      }).pipe(withSpan2("RlmStore.put", { attributes: { ns, key } })),
      putNow: (ns, prefix, data, opts) => {
        const key = prefix + temporalSuffix();
        return gen2(function* () {
          const validNs = validateNamespace(ns);
          const validKey = validateKey(key);
          const meta = data._meta;
          if (meta) validateMeta(meta);
          const summary = meta?.summary;
          const intent = meta?.intent;
          const source = meta?.source;
          const tags = JSON.stringify(opts?.tags ?? []);
          const jsonData = JSON.stringify(data);
          yield* sql`
            INSERT INTO objects (collection, key, data, tags, summary, intent, source, updated_at)
            VALUES (${validNs}, ${validKey}, ${jsonData}, ${tags}, ${summary ?? null}, ${intent ?? null}, ${source ?? null}, datetime('now'))
            ON CONFLICT(collection, key) DO UPDATE SET
              data = excluded.data,
              tags = excluded.tags,
              summary = excluded.summary,
              intent = excluded.intent,
              source = excluded.source,
              updated_at = datetime('now')
          `;
          return { ns: validNs, key: validKey };
        }).pipe(withSpan2("RlmStore.putNow", { attributes: { ns, prefix } }));
      },
      get: (ns, key) => gen2(function* () {
        const rows = yield* sql`
            SELECT data FROM objects WHERE collection = ${ns} AND key = ${key}
          `;
        if (rows.length === 0) return null;
        const parsed = JSON.parse(rows[0].data);
        if (parsed && typeof parsed === "object" && "_meta" in parsed) {
          const { _meta, ...rest } = parsed;
          return rest;
        }
        return parsed;
      }).pipe(withSpan2("RlmStore.get", { attributes: { ns, key } })),
      getRaw: (ns, key) => gen2(function* () {
        const rows = yield* sql`
            SELECT data FROM objects WHERE collection = ${ns} AND key = ${key}
          `;
        if (rows.length === 0) return null;
        return JSON.parse(rows[0].data);
      }).pipe(withSpan2("RlmStore.getRaw", { attributes: { ns, key } })),
      describe: (ns, key) => gen2(function* () {
        const rows = yield* sql`
            SELECT data FROM objects WHERE collection = ${ns} AND key = ${key}
          `;
        if (rows.length === 0) return null;
        const parsed = JSON.parse(rows[0].data);
        return parsed?._meta ?? null;
      }).pipe(withSpan2("RlmStore.describe", { attributes: { ns, key } })),
      del: (ns, key) => gen2(function* () {
        yield* sql`DELETE FROM objects WHERE collection = ${ns} AND key = ${key}`;
        return true;
      }).pipe(withSpan2("RlmStore.del", { attributes: { ns, key } })),
      clear: (ns) => gen2(function* () {
        const rows = yield* sql`SELECT COUNT(*) as cnt FROM objects WHERE collection = ${ns}`;
        const count = rows[0]?.cnt ?? 0;
        yield* sql`DELETE FROM objects WHERE collection = ${ns}`;
        return count;
      }).pipe(withSpan2("RlmStore.clear", { attributes: { ns } })),
      keys: (ns) => gen2(function* () {
        const rows = yield* sql`SELECT key FROM objects WHERE collection = ${ns} ORDER BY key`;
        return rows.map((r) => r.key);
      }).pipe(withSpan2("RlmStore.keys", { attributes: { ns } })),
      query: (ns, filter5) => gen2(function* () {
        let rows;
        if (filter5?.tags && filter5.tags.length > 0) {
          const tagPlaceholders = filter5.tags.map(() => "?").join(", ");
          rows = yield* sql.unsafe(
            `SELECT * FROM objects WHERE collection = ?
               AND (${filter5.tags.map(
              () => `EXISTS (SELECT 1 FROM json_each(tags) WHERE json_each.value = ?)`
            ).join(" AND ")})
               ORDER BY updated_at DESC`,
            [ns, ...filter5.tags]
          );
        } else if (filter5?.jsonPath && filter5?.jsonValue !== void 0) {
          rows = yield* sql.unsafe(
            `SELECT * FROM objects WHERE collection = ?
               AND json_extract(data, ?) = ?
               ORDER BY updated_at DESC`,
            [ns, filter5.jsonPath, filter5.jsonValue]
          );
        } else {
          rows = yield* sql`
              SELECT * FROM objects WHERE collection = ${ns}
              ORDER BY updated_at DESC
            `;
        }
        return rows.map((r) => ({
          collection: r.collection,
          key: r.key,
          data: JSON.parse(r.data),
          tags: JSON.parse(r.tags ?? "[]"),
          created_at: r.created_at,
          updated_at: r.updated_at
        }));
      }).pipe(withSpan2("RlmStore.query", { attributes: { ns } })),
      catalog: (nsGlob) => gen2(function* () {
        const rows = nsGlob ? yield* sql.unsafe(
          `SELECT collection, key, summary, source, intent, tags, created_at, updated_at
                 FROM objects WHERE collection GLOB ?
                 ORDER BY collection, key`,
          [nsGlob.replace(/\*/g, "*")]
        ) : yield* sql`
                SELECT collection, key, summary, source, intent, tags, created_at, updated_at
                FROM objects ORDER BY collection, key
              `;
        return rows.map((r) => ({
          collection: r.collection,
          key: r.key,
          summary: r.summary ?? "",
          source: r.source,
          intent: r.intent,
          tags: JSON.parse(r.tags ?? "[]"),
          created_at: r.created_at,
          updated_at: r.updated_at
        }));
      }).pipe(withSpan2("RlmStore.catalog")),
      collections: (glob) => gen2(function* () {
        const rows = yield* sql`
            SELECT collection as name, COUNT(*) as count
            FROM objects GROUP BY collection ORDER BY collection
          `;
        const all3 = rows.map((r) => ({
          name: r.name,
          count: r.count
        }));
        if (glob) {
          return all3.filter((c) => namespaceMatchesGlob(c.name, glob));
        }
        return all3;
      }).pipe(withSpan2("RlmStore.collections")),
      vars: () => gen2(function* () {
        const rows = yield* sql`
            SELECT collection, key, summary, source, intent, tags, created_at, updated_at
            FROM objects ORDER BY collection, key
          `;
        return rows.map((r) => ({
          collection: r.collection,
          key: r.key,
          summary: r.summary ?? "",
          source: r.source,
          intent: r.intent,
          tags: JSON.parse(r.tags ?? "[]"),
          created_at: r.created_at,
          updated_at: r.updated_at
        }));
      }).pipe(withSpan2("RlmStore.vars")),
      exec: (sqlStr) => gen2(function* () {
        yield* sql.unsafe(sqlStr);
      }).pipe(withSpan2("RlmStore.exec"))
    });
  })
);

// node_modules/effect/dist/Ref.js
var TypeId22 = "~effect/Ref";
var RefProto = {
  [TypeId22]: {
    _A: identity
  },
  ...PipeInspectableProto,
  toJSON() {
    return {
      _id: "Ref",
      ref: this.ref
    };
  }
};
var makeUnsafe5 = (value) => {
  const self2 = Object.create(RefProto);
  self2.ref = make8(value);
  return self2;
};
var make20 = (value) => sync2(() => makeUnsafe5(value));
var get2 = (self2) => sync2(() => self2.ref.current);
var set3 = /* @__PURE__ */ dual(2, (self2, value) => sync2(() => set2(self2.ref, value)));
var update = /* @__PURE__ */ dual(2, (self2, f) => sync2(() => {
  self2.ref.current = f(self2.ref.current);
}));

// node_modules/flexsearch/dist/flexsearch.bundle.module.min.mjs
var w;
function H(a, c, b) {
  const e = typeof b, d = typeof a;
  if (e !== "undefined") {
    if (d !== "undefined") {
      if (b) {
        if (d === "function" && e === d) return function(k) {
          return a(b(k));
        };
        c = a.constructor;
        if (c === b.constructor) {
          if (c === Array) return b.concat(a);
          if (c === Map) {
            var f = new Map(b);
            for (var g of a) f.set(g[0], g[1]);
            return f;
          }
          if (c === Set) {
            g = new Set(b);
            for (f of a.values()) g.add(f);
            return g;
          }
        }
      }
      return a;
    }
    return b;
  }
  return d === "undefined" ? c : a;
}
function aa(a, c) {
  return typeof a === "undefined" ? c : a;
}
function I() {
  return /* @__PURE__ */ Object.create(null);
}
function M(a) {
  return typeof a === "string";
}
function ba(a) {
  return typeof a === "object";
}
function ca(a, c) {
  if (M(c)) a = a[c];
  else for (let b = 0; a && b < c.length; b++) a = a[c[b]];
  return a;
}
var ea = /[^\p{L}\p{N}]+/u;
var fa = /(\d{3})/g;
var ha = /(\D)(\d{3})/g;
var ia = /(\d{3})(\D)/g;
var ja = /[\u0300-\u036f]/g;
function ka(a = {}) {
  if (!this || this.constructor !== ka) return new ka(...arguments);
  if (arguments.length) for (a = 0; a < arguments.length; a++) this.assign(arguments[a]);
  else this.assign(a);
}
w = ka.prototype;
w.assign = function(a) {
  this.normalize = H(a.normalize, true, this.normalize);
  let c = a.include, b = c || a.exclude || a.split, e;
  if (b || b === "") {
    if (typeof b === "object" && b.constructor !== RegExp) {
      let d = "";
      e = !c;
      c || (d += "\\p{Z}");
      b.letter && (d += "\\p{L}");
      b.number && (d += "\\p{N}", e = !!c);
      b.symbol && (d += "\\p{S}");
      b.punctuation && (d += "\\p{P}");
      b.control && (d += "\\p{C}");
      if (b = b.char) d += typeof b === "object" ? b.join("") : b;
      try {
        this.split = new RegExp("[" + (c ? "^" : "") + d + "]+", "u");
      } catch (f) {
        this.split = /\s+/;
      }
    } else this.split = b, e = b === false || "a1a".split(b).length < 2;
    this.numeric = H(a.numeric, e);
  } else {
    try {
      this.split = H(this.split, ea);
    } catch (d) {
      this.split = /\s+/;
    }
    this.numeric = H(a.numeric, H(this.numeric, true));
  }
  this.prepare = H(a.prepare, null, this.prepare);
  this.finalize = H(a.finalize, null, this.finalize);
  b = a.filter;
  this.filter = typeof b === "function" ? b : H(b && new Set(b), null, this.filter);
  this.dedupe = H(a.dedupe, true, this.dedupe);
  this.matcher = H((b = a.matcher) && new Map(b), null, this.matcher);
  this.mapper = H((b = a.mapper) && new Map(b), null, this.mapper);
  this.stemmer = H(
    (b = a.stemmer) && new Map(b),
    null,
    this.stemmer
  );
  this.replacer = H(a.replacer, null, this.replacer);
  this.minlength = H(a.minlength, 1, this.minlength);
  this.maxlength = H(a.maxlength, 1024, this.maxlength);
  this.rtl = H(a.rtl, false, this.rtl);
  if (this.cache = b = H(a.cache, true, this.cache)) this.F = null, this.L = typeof b === "number" ? b : 2e5, this.B = /* @__PURE__ */ new Map(), this.D = /* @__PURE__ */ new Map(), this.I = this.H = 128;
  this.h = "";
  this.J = null;
  this.A = "";
  this.K = null;
  if (this.matcher) for (const d of this.matcher.keys()) this.h += (this.h ? "|" : "") + d;
  if (this.stemmer) for (const d of this.stemmer.keys()) this.A += (this.A ? "|" : "") + d;
  return this;
};
w.addStemmer = function(a, c) {
  this.stemmer || (this.stemmer = /* @__PURE__ */ new Map());
  this.stemmer.set(a, c);
  this.A += (this.A ? "|" : "") + a;
  this.K = null;
  this.cache && Q(this);
  return this;
};
w.addFilter = function(a) {
  typeof a === "function" ? this.filter = a : (this.filter || (this.filter = /* @__PURE__ */ new Set()), this.filter.add(a));
  this.cache && Q(this);
  return this;
};
w.addMapper = function(a, c) {
  if (typeof a === "object") return this.addReplacer(a, c);
  if (a.length > 1) return this.addMatcher(a, c);
  this.mapper || (this.mapper = /* @__PURE__ */ new Map());
  this.mapper.set(a, c);
  this.cache && Q(this);
  return this;
};
w.addMatcher = function(a, c) {
  if (typeof a === "object") return this.addReplacer(a, c);
  if (a.length < 2 && (this.dedupe || this.mapper)) return this.addMapper(a, c);
  this.matcher || (this.matcher = /* @__PURE__ */ new Map());
  this.matcher.set(a, c);
  this.h += (this.h ? "|" : "") + a;
  this.J = null;
  this.cache && Q(this);
  return this;
};
w.addReplacer = function(a, c) {
  if (typeof a === "string") return this.addMatcher(a, c);
  this.replacer || (this.replacer = []);
  this.replacer.push(a, c);
  this.cache && Q(this);
  return this;
};
w.encode = function(a, c) {
  if (this.cache && a.length <= this.H) if (this.F) {
    if (this.B.has(a)) return this.B.get(a);
  } else this.F = setTimeout(Q, 50, this);
  this.normalize && (typeof this.normalize === "function" ? a = this.normalize(a) : a = ja ? a.normalize("NFKD").replace(ja, "").toLowerCase() : a.toLowerCase());
  this.prepare && (a = this.prepare(a));
  this.numeric && a.length > 3 && (a = a.replace(ha, "$1 $2").replace(ia, "$1 $2").replace(fa, "$1 "));
  const b = !(this.dedupe || this.mapper || this.filter || this.matcher || this.stemmer || this.replacer);
  let e = [], d = I(), f, g, k = this.split || this.split === "" ? a.split(this.split) : [a];
  for (let l = 0, m, p; l < k.length; l++) if ((m = p = k[l]) && !(m.length < this.minlength || m.length > this.maxlength)) {
    if (c) {
      if (d[m]) continue;
      d[m] = 1;
    } else {
      if (f === m) continue;
      f = m;
    }
    if (b) e.push(m);
    else if (!this.filter || (typeof this.filter === "function" ? this.filter(m) : !this.filter.has(m))) {
      if (this.cache && m.length <= this.I) if (this.F) {
        var h = this.D.get(m);
        if (h || h === "") {
          h && e.push(h);
          continue;
        }
      } else this.F = setTimeout(Q, 50, this);
      if (this.stemmer) {
        this.K || (this.K = new RegExp("(?!^)(" + this.A + ")$"));
        let u;
        for (; u !== m && m.length > 2; ) u = m, m = m.replace(this.K, (r) => this.stemmer.get(r));
      }
      if (m && (this.mapper || this.dedupe && m.length > 1)) {
        h = "";
        for (let u = 0, r = "", t, n; u < m.length; u++) t = m.charAt(u), t === r && this.dedupe || ((n = this.mapper && this.mapper.get(t)) || n === "" ? n === r && this.dedupe || !(r = n) || (h += n) : h += r = t);
        m = h;
      }
      this.matcher && m.length > 1 && (this.J || (this.J = new RegExp("(" + this.h + ")", "g")), m = m.replace(this.J, (u) => this.matcher.get(u)));
      if (m && this.replacer) for (h = 0; m && h < this.replacer.length; h += 2) m = m.replace(
        this.replacer[h],
        this.replacer[h + 1]
      );
      this.cache && p.length <= this.I && (this.D.set(p, m), this.D.size > this.L && (this.D.clear(), this.I = this.I / 1.1 | 0));
      if (m) {
        if (m !== p) if (c) {
          if (d[m]) continue;
          d[m] = 1;
        } else {
          if (g === m) continue;
          g = m;
        }
        e.push(m);
      }
    }
  }
  this.finalize && (e = this.finalize(e) || e);
  this.cache && a.length <= this.H && (this.B.set(a, e), this.B.size > this.L && (this.B.clear(), this.H = this.H / 1.1 | 0));
  return e;
};
function Q(a) {
  a.F = null;
  a.B.clear();
  a.D.clear();
}
function la(a, c, b) {
  b || (c || typeof a !== "object" ? typeof c === "object" && (b = c, c = 0) : b = a);
  b && (a = b.query || a, c = b.limit || c);
  let e = "" + (c || 0);
  b && (e += (b.offset || 0) + !!b.context + !!b.suggest + (b.resolve !== false) + (b.resolution || this.resolution) + (b.boost || 0));
  a = ("" + a).toLowerCase();
  this.cache || (this.cache = new ma());
  let d = this.cache.get(a + e);
  if (!d) {
    const f = b && b.cache;
    f && (b.cache = false);
    d = this.search(a, c, b);
    f && (b.cache = f);
    this.cache.set(a + e, d);
  }
  return d;
}
function ma(a) {
  this.limit = a && a !== true ? a : 1e3;
  this.cache = /* @__PURE__ */ new Map();
  this.h = "";
}
ma.prototype.set = function(a, c) {
  this.cache.set(this.h = a, c);
  this.cache.size > this.limit && this.cache.delete(this.cache.keys().next().value);
};
ma.prototype.get = function(a) {
  const c = this.cache.get(a);
  c && this.h !== a && (this.cache.delete(a), this.cache.set(this.h = a, c));
  return c;
};
ma.prototype.remove = function(a) {
  for (const c of this.cache) {
    const b = c[0];
    c[1].includes(a) && this.cache.delete(b);
  }
};
ma.prototype.clear = function() {
  this.cache.clear();
  this.h = "";
};
var na = { normalize: false, numeric: false, dedupe: false };
var oa = {};
var ra = /* @__PURE__ */ new Map([["b", "p"], ["v", "f"], ["w", "f"], ["z", "s"], ["x", "s"], ["d", "t"], ["n", "m"], ["c", "k"], ["g", "k"], ["j", "k"], ["q", "k"], ["i", "e"], ["y", "e"], ["u", "o"]]);
var sa = /* @__PURE__ */ new Map([["ae", "a"], ["oe", "o"], ["sh", "s"], ["kh", "k"], ["th", "t"], ["ph", "f"], ["pf", "f"]]);
var ta = [/([^aeo])h(.)/g, "$1$2", /([aeo])h([^aeo]|$)/g, "$1$2", /(.)\1+/g, "$1"];
var ua = { a: "", e: "", i: "", o: "", u: "", y: "", b: 1, f: 1, p: 1, v: 1, c: 2, g: 2, j: 2, k: 2, q: 2, s: 2, x: 2, z: 2, "\xDF": 2, d: 3, t: 3, l: 4, m: 5, n: 5, r: 6 };
var va = { Exact: na, Default: oa, Normalize: oa, LatinBalance: { mapper: ra }, LatinAdvanced: { mapper: ra, matcher: sa, replacer: ta }, LatinExtra: { mapper: ra, replacer: ta.concat([/(?!^)[aeo]/g, ""]), matcher: sa }, LatinSoundex: { dedupe: false, include: { letter: true }, finalize: function(a) {
  for (let b = 0; b < a.length; b++) {
    var c = a[b];
    let e = c.charAt(0), d = ua[e];
    for (let f = 1, g; f < c.length && (g = c.charAt(f), g === "h" || g === "w" || !(g = ua[g]) || g === d || (e += g, d = g, e.length !== 4)); f++) ;
    a[b] = e;
  }
} }, CJK: { split: "" }, LatinExact: na, LatinDefault: oa, LatinSimple: oa };
function wa(a, c, b, e) {
  let d = [];
  for (let f = 0, g; f < a.index.length; f++) if (g = a.index[f], c >= g.length) c -= g.length;
  else {
    c = g[e ? "splice" : "slice"](c, b);
    const k = c.length;
    if (k && (d = d.length ? d.concat(c) : c, b -= k, e && (a.length -= k), !b)) break;
    c = 0;
  }
  return d;
}
function xa(a) {
  if (!this || this.constructor !== xa) return new xa(a);
  this.index = a ? [a] : [];
  this.length = a ? a.length : 0;
  const c = this;
  return new Proxy([], { get(b, e) {
    if (e === "length") return c.length;
    if (e === "push") return function(d) {
      c.index[c.index.length - 1].push(d);
      c.length++;
    };
    if (e === "pop") return function() {
      if (c.length) return c.length--, c.index[c.index.length - 1].pop();
    };
    if (e === "indexOf") return function(d) {
      let f = 0;
      for (let g = 0, k, h; g < c.index.length; g++) {
        k = c.index[g];
        h = k.indexOf(d);
        if (h >= 0) return f + h;
        f += k.length;
      }
      return -1;
    };
    if (e === "includes") return function(d) {
      for (let f = 0; f < c.index.length; f++) if (c.index[f].includes(d)) return true;
      return false;
    };
    if (e === "slice") return function(d, f) {
      return wa(c, d || 0, f || c.length, false);
    };
    if (e === "splice") return function(d, f) {
      return wa(c, d || 0, f || c.length, true);
    };
    if (e === "constructor") return Array;
    if (typeof e !== "symbol") return (b = c.index[e / 2 ** 31 | 0]) && b[e];
  }, set(b, e, d) {
    b = e / 2 ** 31 | 0;
    (c.index[b] || (c.index[b] = []))[e] = d;
    c.length++;
    return true;
  } });
}
xa.prototype.clear = function() {
  this.index.length = 0;
};
xa.prototype.push = function() {
};
function R(a = 8) {
  if (!this || this.constructor !== R) return new R(a);
  this.index = I();
  this.h = [];
  this.size = 0;
  a > 32 ? (this.B = Aa, this.A = BigInt(a)) : (this.B = Ba, this.A = a);
}
R.prototype.get = function(a) {
  const c = this.index[this.B(a)];
  return c && c.get(a);
};
R.prototype.set = function(a, c) {
  var b = this.B(a);
  let e = this.index[b];
  e ? (b = e.size, e.set(a, c), (b -= e.size) && this.size++) : (this.index[b] = e = /* @__PURE__ */ new Map([[a, c]]), this.h.push(e), this.size++);
};
function S(a = 8) {
  if (!this || this.constructor !== S) return new S(a);
  this.index = I();
  this.h = [];
  this.size = 0;
  a > 32 ? (this.B = Aa, this.A = BigInt(a)) : (this.B = Ba, this.A = a);
}
S.prototype.add = function(a) {
  var c = this.B(a);
  let b = this.index[c];
  b ? (c = b.size, b.add(a), (c -= b.size) && this.size++) : (this.index[c] = b = /* @__PURE__ */ new Set([a]), this.h.push(b), this.size++);
};
w = R.prototype;
w.has = S.prototype.has = function(a) {
  const c = this.index[this.B(a)];
  return c && c.has(a);
};
w.delete = S.prototype.delete = function(a) {
  const c = this.index[this.B(a)];
  c && c.delete(a) && this.size--;
};
w.clear = S.prototype.clear = function() {
  this.index = I();
  this.h = [];
  this.size = 0;
};
w.values = S.prototype.values = function* () {
  for (let a = 0; a < this.h.length; a++) for (let c of this.h[a].values()) yield c;
};
w.keys = S.prototype.keys = function* () {
  for (let a = 0; a < this.h.length; a++) for (let c of this.h[a].keys()) yield c;
};
w.entries = S.prototype.entries = function* () {
  for (let a = 0; a < this.h.length; a++) for (let c of this.h[a].entries()) yield c;
};
function Ba(a) {
  let c = 2 ** this.A - 1;
  if (typeof a == "number") return a & c;
  let b = 0, e = this.A + 1;
  for (let d = 0; d < a.length; d++) b = (b * e ^ a.charCodeAt(d)) & c;
  return this.A === 32 ? b + 2 ** 31 : b;
}
function Aa(a) {
  let c = BigInt(2) ** this.A - BigInt(1);
  var b = typeof a;
  if (b === "bigint") return a & c;
  if (b === "number") return BigInt(a) & c;
  b = BigInt(0);
  let e = this.A + BigInt(1);
  for (let d = 0; d < a.length; d++) b = (b * e ^ BigInt(a.charCodeAt(d))) & c;
  return b;
}
var Ca;
var Da;
async function Ea(a) {
  a = a.data;
  var c = a.task;
  const b = a.id;
  let e = a.args;
  switch (c) {
    case "init":
      Da = a.options || {};
      (c = a.factory) ? (Function("return " + c)()(self), Ca = new self.FlexSearch.Index(Da), delete self.FlexSearch) : Ca = new T(Da);
      postMessage({ id: b });
      break;
    default:
      let d;
      c === "export" && (e[1] ? (e[0] = Da.export, e[2] = 0, e[3] = 1) : e = null);
      c === "import" ? e[0] && (a = await Da.import.call(Ca, e[0]), Ca.import(e[0], a)) : ((d = e && Ca[c].apply(Ca, e)) && d.then && (d = await d), d && d.await && (d = await d.await), c === "search" && d.result && (d = d.result));
      postMessage(c === "search" ? { id: b, msg: d } : { id: b });
  }
}
function Fa(a) {
  Ga.call(a, "add");
  Ga.call(a, "append");
  Ga.call(a, "search");
  Ga.call(a, "update");
  Ga.call(a, "remove");
  Ga.call(a, "searchCache");
}
var Ha;
var Ia;
var Ja;
function Ka() {
  Ha = Ja = 0;
}
function Ga(a) {
  this[a + "Async"] = function() {
    const c = arguments;
    var b = c[c.length - 1];
    let e;
    typeof b === "function" && (e = b, delete c[c.length - 1]);
    Ha ? Ja || (Ja = Date.now() - Ia >= this.priority * this.priority * 3) : (Ha = setTimeout(Ka, 0), Ia = Date.now());
    if (Ja) {
      const f = this;
      return new Promise((g) => {
        setTimeout(function() {
          g(f[a + "Async"].apply(f, c));
        }, 0);
      });
    }
    const d = this[a].apply(this, c);
    b = d.then ? d : new Promise((f) => f(d));
    e && b.then(e);
    return b;
  };
}
var V = 0;
function La(a = {}, c) {
  function b(k) {
    function h(l) {
      l = l.data || l;
      const m = l.id, p = m && f.h[m];
      p && (p(l.msg), delete f.h[m]);
    }
    this.worker = k;
    this.h = I();
    if (this.worker) {
      d ? this.worker.on("message", h) : this.worker.onmessage = h;
      if (a.config) return new Promise(function(l) {
        V > 1e9 && (V = 0);
        f.h[++V] = function() {
          l(f);
        };
        f.worker.postMessage({ id: V, task: "init", factory: e, options: a });
      });
      this.priority = a.priority || 4;
      this.encoder = c || null;
      this.worker.postMessage({ task: "init", factory: e, options: a });
      return this;
    }
  }
  if (!this || this.constructor !== La) return new La(a);
  let e = typeof self !== "undefined" ? self._factory : typeof window !== "undefined" ? window._factory : null;
  e && (e = e.toString());
  const d = typeof window === "undefined", f = this, g = Ma(e, d, a.worker);
  return g.then ? g.then(function(k) {
    return b.call(f, k);
  }) : b.call(this, g);
}
W("add");
W("append");
W("search");
W("update");
W("remove");
W("clear");
W("export");
W("import");
La.prototype.searchCache = la;
Fa(La.prototype);
function W(a) {
  La.prototype[a] = function() {
    const c = this, b = [].slice.call(arguments);
    var e = b[b.length - 1];
    let d;
    typeof e === "function" && (d = e, b.pop());
    e = new Promise(function(f) {
      a === "export" && typeof b[0] === "function" && (b[0] = null);
      V > 1e9 && (V = 0);
      c.h[++V] = f;
      c.worker.postMessage({ task: a, id: V, args: b });
    });
    return d ? (e.then(d), this) : e;
  };
}
function Ma(a, c, b) {
  return c ? typeof module !== "undefined" ? new (__require("worker_threads"))["Worker"](__dirname + "/worker/node.js") : import("worker_threads").then(function(worker) {
    return new worker["Worker"](import.meta.dirname + "/node/node.mjs");
  }) : a ? new window.Worker(URL.createObjectURL(new Blob(["onmessage=" + Ea.toString()], { type: "text/javascript" }))) : new window.Worker(typeof b === "string" ? b : import.meta.url.replace("/worker.js", "/worker/worker.js").replace(
    "flexsearch.bundle.module.min.js",
    "module/worker/worker.js"
  ).replace("flexsearch.bundle.module.min.mjs", "module/worker/worker.js"), { type: "module" });
}
Na.prototype.add = function(a, c, b) {
  ba(a) && (c = a, a = ca(c, this.key));
  if (c && (a || a === 0)) {
    if (!b && this.reg.has(a)) return this.update(a, c);
    for (let k = 0, h; k < this.field.length; k++) {
      h = this.B[k];
      var e = this.index.get(this.field[k]);
      if (typeof h === "function") {
        var d = h(c);
        d && e.add(a, d, b, true);
      } else if (d = h.G, !d || d(c)) h.constructor === String ? h = ["" + h] : M(h) && (h = [h]), Qa(c, h, this.D, 0, e, a, h[0], b);
    }
    if (this.tag) for (e = 0; e < this.A.length; e++) {
      var f = this.A[e];
      d = this.tag.get(this.F[e]);
      let k = I();
      if (typeof f === "function") {
        if (f = f(c), !f) continue;
      } else {
        var g = f.G;
        if (g && !g(c)) continue;
        f.constructor === String && (f = "" + f);
        f = ca(c, f);
      }
      if (d && f) {
        M(f) && (f = [f]);
        for (let h = 0, l, m; h < f.length; h++) if (l = f[h], !k[l] && (k[l] = 1, (g = d.get(l)) ? m = g : d.set(l, m = []), !b || !m.includes(a))) {
          if (m.length === 2 ** 31 - 1) {
            g = new xa(m);
            if (this.fastupdate) for (let p of this.reg.values()) p.includes(m) && (p[p.indexOf(m)] = g);
            d.set(l, m = g);
          }
          m.push(a);
          this.fastupdate && ((g = this.reg.get(a)) ? g.push(m) : this.reg.set(a, [m]));
        }
      }
    }
    if (this.store && (!b || !this.store.has(a))) {
      let k;
      if (this.h) {
        k = I();
        for (let h = 0, l; h < this.h.length; h++) {
          l = this.h[h];
          if ((b = l.G) && !b(c)) continue;
          let m;
          if (typeof l === "function") {
            m = l(c);
            if (!m) continue;
            l = [l.O];
          } else if (M(l) || l.constructor === String) {
            k[l] = c[l];
            continue;
          }
          Ra(c, k, l, 0, l[0], m);
        }
      }
      this.store.set(a, k || c);
    }
    this.worker && (this.fastupdate || this.reg.add(a));
  }
  return this;
};
function Ra(a, c, b, e, d, f) {
  a = a[d];
  if (e === b.length - 1) c[d] = f || a;
  else if (a) if (a.constructor === Array) for (c = c[d] = Array(a.length), d = 0; d < a.length; d++) Ra(a, c, b, e, d);
  else c = c[d] || (c[d] = I()), d = b[++e], Ra(a, c, b, e, d);
}
function Qa(a, c, b, e, d, f, g, k) {
  if (a = a[g]) if (e === c.length - 1) {
    if (a.constructor === Array) {
      if (b[e]) {
        for (c = 0; c < a.length; c++) d.add(f, a[c], true, true);
        return;
      }
      a = a.join(" ");
    }
    d.add(f, a, k, true);
  } else if (a.constructor === Array) for (g = 0; g < a.length; g++) Qa(a, c, b, e, d, f, g, k);
  else g = c[++e], Qa(a, c, b, e, d, f, g, k);
}
function Sa(a, c, b, e) {
  if (!a.length) return a;
  if (a.length === 1) return a = a[0], a = b || a.length > c ? a.slice(b, b + c) : a, e ? Ta.call(this, a) : a;
  let d = [];
  for (let f = 0, g, k; f < a.length; f++) if ((g = a[f]) && (k = g.length)) {
    if (b) {
      if (b >= k) {
        b -= k;
        continue;
      }
      g = g.slice(b, b + c);
      k = g.length;
      b = 0;
    }
    k > c && (g = g.slice(0, c), k = c);
    if (!d.length && k >= c) return e ? Ta.call(this, g) : g;
    d.push(g);
    c -= k;
    if (!c) break;
  }
  d = d.length > 1 ? [].concat.apply([], d) : d[0];
  return e ? Ta.call(this, d) : d;
}
function Ua(a, c, b, e) {
  var d = e[0];
  if (d[0] && d[0].query) return a[c].apply(a, d);
  if (!(c !== "and" && c !== "not" || a.result.length || a.await || d.suggest)) return e.length > 1 && (d = e[e.length - 1]), (e = d.resolve) ? a.await || a.result : a;
  let f = [], g = 0, k = 0, h, l, m, p, u;
  for (c = 0; c < e.length; c++) if (d = e[c]) {
    var r = void 0;
    if (d.constructor === X) r = d.await || d.result;
    else if (d.then || d.constructor === Array) r = d;
    else {
      g = d.limit || 0;
      k = d.offset || 0;
      m = d.suggest;
      l = d.resolve;
      h = ((p = d.highlight || a.highlight) || d.enrich) && l;
      r = d.queue;
      let t = d.async || r, n = d.index, q = d.query;
      n ? a.index || (a.index = n) : n = a.index;
      if (q || d.tag) {
        const x = d.field || d.pluck;
        x && (!q || a.query && !p || (a.query = q, a.field = x, a.highlight = p), n = n.index.get(x));
        if (r && (u || a.await)) {
          u = 1;
          let v;
          const A = a.C.length, D = new Promise(function(F) {
            v = F;
          });
          (function(F, E) {
            D.h = function() {
              E.index = null;
              E.resolve = false;
              let B = t ? F.searchAsync(E) : F.search(E);
              if (B.then) return B.then(function(z) {
                a.C[A] = z = z.result || z;
                v(z);
                return z;
              });
              B = B.result || B;
              v(B);
              return B;
            };
          })(n, Object.assign({}, d));
          a.C.push(D);
          f[c] = D;
          continue;
        } else d.resolve = false, d.index = null, r = t ? n.searchAsync(d) : n.search(d), d.resolve = l, d.index = n;
      } else if (d.and) r = Va(d, "and", n);
      else if (d.or) r = Va(d, "or", n);
      else if (d.not) r = Va(d, "not", n);
      else if (d.xor) r = Va(d, "xor", n);
      else continue;
    }
    r.await ? (u = 1, r = r.await) : r.then ? (u = 1, r = r.then(function(t) {
      return t.result || t;
    })) : r = r.result || r;
    f[c] = r;
  }
  u && !a.await && (a.await = new Promise(function(t) {
    a.return = t;
  }));
  if (u) {
    const t = Promise.all(f).then(function(n) {
      for (let q = 0; q < a.C.length; q++) if (a.C[q] === t) {
        a.C[q] = function() {
          return b.call(a, n, g, k, h, l, m, p);
        };
        break;
      }
      Wa(a);
    });
    a.C.push(t);
  } else if (a.await) a.C.push(function() {
    return b.call(a, f, g, k, h, l, m, p);
  });
  else return b.call(a, f, g, k, h, l, m, p);
  return l ? a.await || a.result : a;
}
function Va(a, c, b) {
  a = a[c];
  const e = a[0] || a;
  e.index || (e.index = b);
  b = new X(e);
  a.length > 1 && (b = b[c].apply(b, a.slice(1)));
  return b;
}
X.prototype.or = function() {
  return Ua(this, "or", Xa, arguments);
};
function Xa(a, c, b, e, d, f, g) {
  a.length && (this.result.length && a.push(this.result), a.length < 2 ? this.result = a[0] : (this.result = Ya(a, c, b, false, this.h), b = 0));
  d && (this.await = null);
  return d ? this.resolve(c, b, e, g) : this;
}
X.prototype.and = function() {
  return Ua(this, "and", Za, arguments);
};
function Za(a, c, b, e, d, f, g) {
  if (!f && !this.result.length) return d ? this.result : this;
  let k;
  if (a.length) if (this.result.length && a.unshift(this.result), a.length < 2) this.result = a[0];
  else {
    let h = 0;
    for (let l = 0, m, p; l < a.length; l++) if ((m = a[l]) && (p = m.length)) h < p && (h = p);
    else if (!f) {
      h = 0;
      break;
    }
    h ? (this.result = $a(a, h, c, b, f, this.h, d), k = true) : this.result = [];
  }
  else f || (this.result = a);
  d && (this.await = null);
  return d ? this.resolve(c, b, e, g, k) : this;
}
X.prototype.xor = function() {
  return Ua(this, "xor", ab, arguments);
};
function ab(a, c, b, e, d, f, g) {
  if (a.length) if (this.result.length && a.unshift(this.result), a.length < 2) this.result = a[0];
  else {
    a: {
      f = b;
      var k = this.h;
      const h = [], l = I();
      let m = 0;
      for (let p = 0, u; p < a.length; p++) if (u = a[p]) {
        m < u.length && (m = u.length);
        for (let r = 0, t; r < u.length; r++) if (t = u[r]) for (let n = 0, q; n < t.length; n++) q = t[n], l[q] = l[q] ? 2 : 1;
      }
      for (let p = 0, u, r = 0; p < m; p++) for (let t = 0, n; t < a.length; t++) if (n = a[t]) {
        if (u = n[p]) {
          for (let q = 0, x; q < u.length; q++) if (x = u[q], l[x] === 1) if (f) f--;
          else if (d) {
            if (h.push(x), h.length === c) {
              a = h;
              break a;
            }
          } else {
            const v = p + (t ? k : 0);
            h[v] || (h[v] = []);
            h[v].push(x);
            if (++r === c) {
              a = h;
              break a;
            }
          }
        }
      }
      a = h;
    }
    this.result = a;
    k = true;
  }
  else f || (this.result = a);
  d && (this.await = null);
  return d ? this.resolve(c, b, e, g, k) : this;
}
X.prototype.not = function() {
  return Ua(this, "not", bb, arguments);
};
function bb(a, c, b, e, d, f, g) {
  if (!f && !this.result.length) return d ? this.result : this;
  if (a.length && this.result.length) {
    a: {
      f = b;
      var k = [];
      a = new Set(a.flat().flat());
      for (let h = 0, l, m = 0; h < this.result.length; h++) if (l = this.result[h]) {
        for (let p = 0, u; p < l.length; p++) if (u = l[p], !a.has(u)) {
          if (f) f--;
          else if (d) {
            if (k.push(u), k.length === c) {
              a = k;
              break a;
            }
          } else if (k[h] || (k[h] = []), k[h].push(u), ++m === c) {
            a = k;
            break a;
          }
        }
      }
      a = k;
    }
    this.result = a;
    k = true;
  }
  d && (this.await = null);
  return d ? this.resolve(c, b, e, g, k) : this;
}
function cb(a, c, b, e, d) {
  let f, g, k;
  typeof d === "string" ? (f = d, d = "") : f = d.template;
  g = f.indexOf("$1");
  k = f.substring(g + 2);
  g = f.substring(0, g);
  let h = d && d.boundary, l = !d || d.clip !== false, m = d && d.merge && k && g && new RegExp(k + " " + g, "g");
  d = d && d.ellipsis;
  var p = 0;
  if (typeof d === "object") {
    var u = d.template;
    p = u.length - 2;
    d = d.pattern;
  }
  typeof d !== "string" && (d = d === false ? "" : "...");
  p && (d = u.replace("$1", d));
  u = d.length - p;
  let r, t;
  typeof h === "object" && (r = h.before, r === 0 && (r = -1), t = h.after, t === 0 && (t = -1), h = h.total || 9e5);
  p = /* @__PURE__ */ new Map();
  for (let Oa = 0, da, db, pa; Oa < c.length; Oa++) {
    let qa;
    if (e) qa = c, pa = e;
    else {
      var n = c[Oa];
      pa = n.field;
      if (!pa) continue;
      qa = n.result;
    }
    db = b.get(pa);
    da = db.encoder;
    n = p.get(da);
    typeof n !== "string" && (n = da.encode(a), p.set(da, n));
    for (let ya = 0; ya < qa.length; ya++) {
      var q = qa[ya].doc;
      if (!q) continue;
      q = ca(q, pa);
      if (!q) continue;
      var x = q.trim().split(/\s+/);
      if (!x.length) continue;
      q = "";
      var v = [];
      let za = [];
      var A = -1, D = -1, F = 0;
      for (var E = 0; E < x.length; E++) {
        var B = x[E], z = da.encode(B);
        z = z.length > 1 ? z.join(" ") : z[0];
        let y;
        if (z && B) {
          var C = B.length, J = (da.split ? B.replace(da.split, "") : B).length - z.length, G = "", N = 0;
          for (var O = 0; O < n.length; O++) {
            var P = n[O];
            if (P) {
              var L = P.length;
              L += J < 0 ? 0 : J;
              N && L <= N || (P = z.indexOf(P), P > -1 && (G = (P ? B.substring(0, P) : "") + g + B.substring(P, P + L) + k + (P + L < C ? B.substring(P + L) : ""), N = L, y = true));
            }
          }
          G && (h && (A < 0 && (A = q.length + (q ? 1 : 0)), D = q.length + (q ? 1 : 0) + G.length, F += C, za.push(v.length), v.push({ match: G })), q += (q ? " " : "") + G);
        }
        if (!y) B = x[E], q += (q ? " " : "") + B, h && v.push({ text: B });
        else if (h && F >= h) break;
      }
      F = za.length * (f.length - 2);
      if (r || t || h && q.length - F > h) if (F = h + F - u * 2, E = D - A, r > 0 && (E += r), t > 0 && (E += t), E <= F) x = r ? A - (r > 0 ? r : 0) : A - ((F - E) / 2 | 0), v = t ? D + (t > 0 ? t : 0) : x + F, l || (x > 0 && q.charAt(x) !== " " && q.charAt(x - 1) !== " " && (x = q.indexOf(" ", x), x < 0 && (x = 0)), v < q.length && q.charAt(v - 1) !== " " && q.charAt(v) !== " " && (v = q.lastIndexOf(" ", v), v < D ? v = D : ++v)), q = (x ? d : "") + q.substring(x, v) + (v < q.length ? d : "");
      else {
        D = [];
        A = {};
        F = {};
        E = {};
        B = {};
        z = {};
        G = J = C = 0;
        for (O = N = 1; ; ) {
          var U = void 0;
          for (let y = 0, K; y < za.length; y++) {
            K = za[y];
            if (G) if (J !== G) {
              if (E[y + 1]) continue;
              K += G;
              if (A[K]) {
                C -= u;
                F[y + 1] = 1;
                E[y + 1] = 1;
                continue;
              }
              if (K >= v.length - 1) {
                if (K >= v.length) {
                  E[y + 1] = 1;
                  K >= x.length && (F[y + 1] = 1);
                  continue;
                }
                C -= u;
              }
              q = v[K].text;
              if (L = t && z[y]) if (L > 0) {
                if (q.length > L) if (E[y + 1] = 1, l) q = q.substring(0, L);
                else continue;
                (L -= q.length) || (L = -1);
                z[y] = L;
              } else {
                E[y + 1] = 1;
                continue;
              }
              if (C + q.length + 1 <= h) q = " " + q, D[y] += q;
              else if (l) U = h - C - 1, U > 0 && (q = " " + q.substring(0, U), D[y] += q), E[y + 1] = 1;
              else {
                E[y + 1] = 1;
                continue;
              }
            } else {
              if (E[y]) continue;
              K -= J;
              if (A[K]) {
                C -= u;
                E[y] = 1;
                F[y] = 1;
                continue;
              }
              if (K <= 0) {
                if (K < 0) {
                  E[y] = 1;
                  F[y] = 1;
                  continue;
                }
                C -= u;
              }
              q = v[K].text;
              if (L = r && B[y]) if (L > 0) {
                if (q.length > L) if (E[y] = 1, l) q = q.substring(q.length - L);
                else continue;
                (L -= q.length) || (L = -1);
                B[y] = L;
              } else {
                E[y] = 1;
                continue;
              }
              if (C + q.length + 1 <= h) q += " ", D[y] = q + D[y];
              else if (l) U = q.length + 1 - (h - C), U >= 0 && U < q.length && (q = q.substring(U) + " ", D[y] = q + D[y]), E[y] = 1;
              else {
                E[y] = 1;
                continue;
              }
            }
            else {
              q = v[K].match;
              r && (B[y] = r);
              t && (z[y] = t);
              y && C++;
              let Pa;
              K ? !y && u && (C += u) : (F[y] = 1, E[y] = 1);
              K >= x.length - 1 ? Pa = 1 : K < v.length - 1 && v[K + 1].match ? Pa = 1 : u && (C += u);
              C -= f.length - 2;
              if (!y || C + q.length <= h) D[y] = q;
              else {
                U = N = O = F[y] = 0;
                break;
              }
              Pa && (F[y + 1] = 1, E[y + 1] = 1);
            }
            C += q.length;
            U = A[K] = 1;
          }
          if (U) J === G ? G++ : J++;
          else {
            J === G ? N = 0 : O = 0;
            if (!N && !O) break;
            N ? (J++, G = J) : G++;
          }
        }
        q = "";
        for (let y = 0, K; y < D.length; y++) K = (F[y] ? y ? " " : "" : (y && !d ? " " : "") + d) + D[y], q += K;
        d && !F[D.length] && (q += d);
      }
      m && (q = q.replace(m, " "));
      qa[ya].highlight = q;
    }
    if (e) break;
  }
  return c;
}
function X(a, c) {
  if (!this || this.constructor !== X) return new X(a, c);
  let b = 0, e, d, f, g, k, h;
  if (a && a.index) {
    const l = a;
    c = l.index;
    b = l.boost || 0;
    if (d = l.query) {
      f = l.field || l.pluck;
      g = l.highlight;
      const m = l.resolve;
      a = l.async || l.queue;
      l.resolve = false;
      l.index = null;
      a = a ? c.searchAsync(l) : c.search(l);
      l.resolve = m;
      l.index = c;
      a = a.result || a;
    } else a = [];
  }
  if (a && a.then) {
    const l = this;
    a = a.then(function(m) {
      l.C[0] = l.result = m.result || m;
      Wa(l);
    });
    e = [a];
    a = [];
    k = new Promise(function(m) {
      h = m;
    });
  }
  this.index = c || null;
  this.result = a || [];
  this.h = b;
  this.C = e || [];
  this.await = k || null;
  this.return = h || null;
  this.highlight = g || null;
  this.query = d || "";
  this.field = f || "";
}
w = X.prototype;
w.limit = function(a) {
  if (this.await) {
    const c = this;
    this.C.push(function() {
      return c.limit(a).result;
    });
  } else if (this.result.length) {
    const c = [];
    for (let b = 0, e; b < this.result.length; b++) if (e = this.result[b]) if (e.length <= a) {
      if (c[b] = e, a -= e.length, !a) break;
    } else {
      c[b] = e.slice(0, a);
      break;
    }
    this.result = c;
  }
  return this;
};
w.offset = function(a) {
  if (this.await) {
    const c = this;
    this.C.push(function() {
      return c.offset(a).result;
    });
  } else if (this.result.length) {
    const c = [];
    for (let b = 0, e; b < this.result.length; b++) if (e = this.result[b]) e.length <= a ? a -= e.length : (c[b] = e.slice(a), a = 0);
    this.result = c;
  }
  return this;
};
w.boost = function(a) {
  if (this.await) {
    const c = this;
    this.C.push(function() {
      return c.boost(a).result;
    });
  } else this.h += a;
  return this;
};
function Wa(a, c) {
  let b = a.result;
  var e = a.await;
  a.await = null;
  for (let d = 0, f; d < a.C.length; d++) if (f = a.C[d]) {
    if (typeof f === "function") b = f(), a.C[d] = b = b.result || b, d--;
    else if (f.h) b = f.h(), a.C[d] = b = b.result || b, d--;
    else if (f.then) return a.await = e;
  }
  e = a.return;
  a.C = [];
  a.return = null;
  c || e(b);
  return b;
}
w.resolve = function(a, c, b, e, d) {
  let f = this.await ? Wa(this, true) : this.result;
  if (f.then) {
    const g = this;
    return f.then(function() {
      return g.resolve(a, c, b, e, d);
    });
  }
  f.length && (typeof a === "object" ? (e = a.highlight || this.highlight, b = !!e || a.enrich, c = a.offset, a = a.limit) : (e = e || this.highlight, b = !!e || b), f = d ? b ? Ta.call(this.index, f) : f : Sa.call(this.index, f, a || 100, c, b));
  return this.finalize(f, e);
};
w.finalize = function(a, c) {
  if (a.then) {
    const e = this;
    return a.then(function(d) {
      return e.finalize(d, c);
    });
  }
  c && a.length && this.query && (a = cb(this.query, a, this.index.index, this.field, c));
  const b = this.return;
  this.highlight = this.index = this.result = this.C = this.await = this.return = null;
  this.query = this.field = "";
  b && b(a);
  return a;
};
function $a(a, c, b, e, d, f, g) {
  const k = a.length;
  let h = [], l, m;
  l = I();
  for (let p = 0, u, r, t, n; p < c; p++) for (let q = 0; q < k; q++) if (t = a[q], p < t.length && (u = t[p])) for (let x = 0; x < u.length; x++) {
    r = u[x];
    (m = l[r]) ? l[r]++ : (m = 0, l[r] = 1);
    n = h[m] || (h[m] = []);
    if (!g) {
      let v = p + (q || !d ? 0 : f || 0);
      n = n[v] || (n[v] = []);
    }
    n.push(r);
    if (g && b && m === k - 1 && n.length - e === b) return e ? n.slice(e) : n;
  }
  if (a = h.length) if (d) h = h.length > 1 ? Ya(h, b, e, g, f) : (h = h[0]) && b && h.length > b || e ? h.slice(e, b + e) : h;
  else {
    if (a < k) return [];
    h = h[a - 1];
    if (b || e) if (g) {
      if (h.length > b || e) h = h.slice(e, b + e);
    } else {
      d = [];
      for (let p = 0, u; p < h.length; p++) if (u = h[p]) if (e && u.length > e) e -= u.length;
      else {
        if (b && u.length > b || e) u = u.slice(e, b + e), b -= u.length, e && (e -= u.length);
        d.push(u);
        if (!b) break;
      }
      h = d;
    }
  }
  return h;
}
function Ya(a, c, b, e, d) {
  const f = [], g = I();
  let k;
  var h = a.length;
  let l;
  if (e) for (d = h - 1; d >= 0; d--) {
    if (l = (e = a[d]) && e.length) {
      for (h = 0; h < l; h++) if (k = e[h], !g[k]) {
        if (g[k] = 1, b) b--;
        else if (f.push(k), f.length === c) return f;
      }
    }
  }
  else for (let m = h - 1, p, u = 0; m >= 0; m--) {
    p = a[m];
    for (let r = 0; r < p.length; r++) if (l = (e = p[r]) && e.length) {
      for (let t = 0; t < l; t++) if (k = e[t], !g[k]) if (g[k] = 1, b) b--;
      else {
        let n = (r + (m < h - 1 ? d || 0 : 0)) / (m + 1) | 0;
        (f[n] || (f[n] = [])).push(k);
        if (++u === c) return f;
      }
    }
  }
  return f;
}
function eb(a, c, b, e, d) {
  const f = I(), g = [];
  for (let k = 0, h; k < c.length; k++) {
    h = c[k];
    for (let l = 0; l < h.length; l++) f[h[l]] = 1;
  }
  if (d) for (let k = 0, h; k < a.length; k++) {
    if (h = a[k], f[h]) {
      if (e) e--;
      else if (g.push(h), f[h] = 0, b && --b === 0) break;
    }
  }
  else for (let k = 0, h, l; k < a.result.length; k++) for (h = a.result[k], c = 0; c < h.length; c++) l = h[c], f[l] && ((g[k] || (g[k] = [])).push(l), f[l] = 0);
  return g;
}
I();
Na.prototype.search = function(a, c, b, e) {
  b || (!c && ba(a) ? (b = a, a = "") : ba(c) && (b = c, c = 0));
  let d = [];
  var f = [];
  let g;
  let k, h, l, m, p;
  let u = 0, r = true, t;
  if (b) {
    b.constructor === Array && (b = { index: b });
    a = b.query || a;
    g = b.pluck;
    k = b.merge;
    l = b.boost;
    p = g || b.field || (p = b.index) && (p.index ? null : p);
    var n = this.tag && b.tag;
    h = b.suggest;
    r = b.resolve !== false;
    m = b.cache;
    t = r && this.store && b.highlight;
    var q = !!t || r && this.store && b.enrich;
    c = b.limit || c;
    var x = b.offset || 0;
    c || (c = r ? 100 : 0);
    if (n && (!this.db || !e)) {
      n.constructor !== Array && (n = [n]);
      var v = [];
      for (let B = 0, z; B < n.length; B++) if (z = n[B], z.field && z.tag) {
        var A = z.tag;
        if (A.constructor === Array) for (var D = 0; D < A.length; D++) v.push(z.field, A[D]);
        else v.push(z.field, A);
      } else {
        A = Object.keys(z);
        for (let C = 0, J, G; C < A.length; C++) if (J = A[C], G = z[J], G.constructor === Array) for (D = 0; D < G.length; D++) v.push(J, G[D]);
        else v.push(J, G);
      }
      n = v;
      if (!a) {
        f = [];
        if (v.length) for (n = 0; n < v.length; n += 2) {
          if (this.db) {
            e = this.index.get(v[n]);
            if (!e) continue;
            f.push(e = e.db.tag(v[n + 1], c, x, q));
          } else e = fb.call(this, v[n], v[n + 1], c, x, q);
          d.push(r ? { field: v[n], tag: v[n + 1], result: e } : [e]);
        }
        if (f.length) {
          const B = this;
          return Promise.all(f).then(function(z) {
            for (let C = 0; C < z.length; C++) r ? d[C].result = z[C] : d[C] = z[C];
            return r ? d : new X(d.length > 1 ? $a(d, 1, 0, 0, h, l) : d[0], B);
          });
        }
        return r ? d : new X(d.length > 1 ? $a(d, 1, 0, 0, h, l) : d[0], this);
      }
    }
    r || g || !(p = p || this.field) || (M(p) ? g = p : (p.constructor === Array && p.length === 1 && (p = p[0]), g = p.field || p.index));
    p && p.constructor !== Array && (p = [p]);
  }
  p || (p = this.field);
  let F;
  v = (this.worker || this.db) && !e && [];
  for (let B = 0, z, C, J; B < p.length; B++) {
    C = p[B];
    if (this.db && this.tag && !this.B[B]) continue;
    let G;
    M(C) || (G = C, C = G.field, a = G.query || a, c = aa(G.limit, c), x = aa(G.offset, x), h = aa(G.suggest, h), t = r && this.store && aa(G.highlight, t), q = !!t || r && this.store && aa(G.enrich, q), m = aa(G.cache, m));
    if (e) z = e[B];
    else {
      A = G || b || {};
      D = A.enrich;
      var E = this.index.get(C);
      n && (this.db && (A.tag = n, A.field = p, F = E.db.support_tag_search), !F && D && (A.enrich = false), F || (A.limit = 0, A.offset = 0));
      z = m ? E.searchCache(a, n && !F ? 0 : c, A) : E.search(a, n && !F ? 0 : c, A);
      n && !F && (A.limit = c, A.offset = x);
      D && (A.enrich = D);
      if (v) {
        v[B] = z;
        continue;
      }
    }
    J = (z = z.result || z) && z.length;
    if (n && J) {
      A = [];
      D = 0;
      if (this.db && e) {
        if (!F) for (E = p.length; E < e.length; E++) {
          let N = e[E];
          if (N && N.length) D++, A.push(N);
          else if (!h) return r ? d : new X(d, this);
        }
      } else for (let N = 0, O, P; N < n.length; N += 2) {
        O = this.tag.get(n[N]);
        if (!O) if (h) continue;
        else return r ? d : new X(d, this);
        if (P = (O = O && O.get(n[N + 1])) && O.length) D++, A.push(O);
        else if (!h) return r ? d : new X(d, this);
      }
      if (D) {
        z = eb(z, A, c, x, r);
        J = z.length;
        if (!J && !h) return r ? z : new X(z, this);
        D--;
      }
    }
    if (J) f[u] = C, d.push(z), u++;
    else if (p.length === 1) return r ? d : new X(
      d,
      this
    );
  }
  if (v) {
    if (this.db && n && n.length && !F) for (q = 0; q < n.length; q += 2) {
      f = this.index.get(n[q]);
      if (!f) if (h) continue;
      else return r ? d : new X(d, this);
      v.push(f.db.tag(n[q + 1], c, x, false));
    }
    const B = this;
    return Promise.all(v).then(function(z) {
      b && (b.resolve = r);
      z.length && (z = B.search(a, c, b, z));
      return z;
    });
  }
  if (!u) return r ? d : new X(d, this);
  if (g && (!q || !this.store)) return d = d[0], r ? d : new X(d, this);
  v = [];
  for (x = 0; x < f.length; x++) {
    n = d[x];
    q && n.length && typeof n[0].doc === "undefined" && (this.db ? v.push(n = this.index.get(this.field[0]).db.enrich(n)) : n = Ta.call(this, n));
    if (g) return r ? t ? cb(a, n, this.index, g, t) : n : new X(n, this);
    d[x] = { field: f[x], result: n };
  }
  if (q && this.db && v.length) {
    const B = this;
    return Promise.all(v).then(function(z) {
      for (let C = 0; C < z.length; C++) d[C].result = z[C];
      t && (d = cb(a, d, B.index, g, t));
      return k ? gb(d) : d;
    });
  }
  t && (d = cb(a, d, this.index, g, t));
  return k ? gb(d) : d;
};
function gb(a) {
  const c = [], b = I(), e = I();
  for (let d = 0, f, g, k, h, l, m, p; d < a.length; d++) {
    f = a[d];
    g = f.field;
    k = f.result;
    for (let u = 0; u < k.length; u++) if (l = k[u], typeof l !== "object" ? l = { id: h = l } : h = l.id, (m = b[h]) ? m.push(g) : (l.field = b[h] = [g], c.push(l)), p = l.highlight) m = e[h], m || (e[h] = m = {}, l.highlight = m), m[g] = p;
  }
  return c;
}
function fb(a, c, b, e, d) {
  a = this.tag.get(a);
  if (!a) return [];
  a = a.get(c);
  if (!a) return [];
  c = a.length - e;
  if (c > 0) {
    if (b && c > b || e) a = a.slice(e, e + b);
    d && (a = Ta.call(this, a));
  }
  return a;
}
function Ta(a) {
  if (!this || !this.store) return a;
  if (this.db) return this.index.get(this.field[0]).db.enrich(a);
  const c = Array(a.length);
  for (let b = 0, e; b < a.length; b++) e = a[b], c[b] = { id: e, doc: this.store.get(e) };
  return c;
}
function Na(a) {
  if (!this || this.constructor !== Na) return new Na(a);
  const c = a.document || a.doc || a;
  let b, e;
  this.B = [];
  this.field = [];
  this.D = [];
  this.key = (b = c.key || c.id) && hb(b, this.D) || "id";
  (e = a.keystore || 0) && (this.keystore = e);
  this.fastupdate = !!a.fastupdate;
  this.reg = !this.fastupdate || a.worker || a.db ? e ? new S(e) : /* @__PURE__ */ new Set() : e ? new R(e) : /* @__PURE__ */ new Map();
  this.h = (b = c.store || null) && b && b !== true && [];
  this.store = b ? e ? new R(e) : /* @__PURE__ */ new Map() : null;
  this.cache = (b = a.cache || null) && new ma(b);
  a.cache = false;
  this.worker = a.worker || false;
  this.priority = a.priority || 4;
  this.index = ib.call(this, a, c);
  this.tag = null;
  if (b = c.tag) {
    if (typeof b === "string" && (b = [b]), b.length) {
      this.tag = /* @__PURE__ */ new Map();
      this.A = [];
      this.F = [];
      for (let d = 0, f, g; d < b.length; d++) {
        f = b[d];
        g = f.field || f;
        if (!g) throw Error("The tag field from the document descriptor is undefined.");
        f.custom ? this.A[d] = f.custom : (this.A[d] = hb(g, this.D), f.filter && (typeof this.A[d] === "string" && (this.A[d] = new String(this.A[d])), this.A[d].G = f.filter));
        this.F[d] = g;
        this.tag.set(g, /* @__PURE__ */ new Map());
      }
    }
  }
  if (this.worker) {
    this.fastupdate = false;
    a = [];
    for (const d of this.index.values()) d.then && a.push(d);
    if (a.length) {
      const d = this;
      return Promise.all(a).then(function(f) {
        let g = 0;
        for (const k of d.index.entries()) {
          const h = k[0];
          let l = k[1];
          l.then && (l = f[g], d.index.set(h, l), g++);
        }
        return d;
      });
    }
  } else a.db && (this.fastupdate = false, this.mount(a.db));
}
w = Na.prototype;
w.mount = function(a) {
  let c = this.field;
  if (this.tag) for (let f = 0, g; f < this.F.length; f++) {
    g = this.F[f];
    var b = void 0;
    this.index.set(g, b = new T({}, this.reg));
    c === this.field && (c = c.slice(0));
    c.push(g);
    b.tag = this.tag.get(g);
  }
  b = [];
  const e = { db: a.db, type: a.type, fastupdate: a.fastupdate };
  for (let f = 0, g, k; f < c.length; f++) {
    e.field = k = c[f];
    g = this.index.get(k);
    const h = new a.constructor(a.id, e);
    h.id = a.id;
    b[f] = h.mount(g);
    g.document = true;
    f ? g.bypass = true : g.store = this.store;
  }
  const d = this;
  return this.db = Promise.all(b).then(function() {
    d.db = true;
  });
};
w.commit = async function() {
  const a = [];
  for (const c of this.index.values()) a.push(c.commit());
  await Promise.all(a);
  this.reg.clear();
};
w.destroy = function() {
  const a = [];
  for (const c of this.index.values()) a.push(c.destroy());
  return Promise.all(a);
};
function ib(a, c) {
  const b = /* @__PURE__ */ new Map();
  let e = c.index || c.field || c;
  M(e) && (e = [e]);
  for (let f = 0, g, k; f < e.length; f++) {
    g = e[f];
    M(g) || (k = g, g = g.field);
    k = ba(k) ? Object.assign({}, a, k) : a;
    if (this.worker) {
      var d = void 0;
      d = (d = k.encoder) && d.encode ? d : new ka(typeof d === "string" ? va[d] : d || {});
      d = new La(k, d);
      b.set(g, d);
    }
    this.worker || b.set(g, new T(k, this.reg));
    k.custom ? this.B[f] = k.custom : (this.B[f] = hb(g, this.D), k.filter && (typeof this.B[f] === "string" && (this.B[f] = new String(this.B[f])), this.B[f].G = k.filter));
    this.field[f] = g;
  }
  if (this.h) {
    a = c.store;
    M(a) && (a = [a]);
    for (let f = 0, g, k; f < a.length; f++) g = a[f], k = g.field || g, g.custom ? (this.h[f] = g.custom, g.custom.O = k) : (this.h[f] = hb(k, this.D), g.filter && (typeof this.h[f] === "string" && (this.h[f] = new String(this.h[f])), this.h[f].G = g.filter));
  }
  return b;
}
function hb(a, c) {
  const b = a.split(":");
  let e = 0;
  for (let d = 0; d < b.length; d++) a = b[d], a[a.length - 1] === "]" && (a = a.substring(0, a.length - 2)) && (c[e] = true), a && (b[e++] = a);
  e < b.length && (b.length = e);
  return e > 1 ? b : b[0];
}
w.append = function(a, c) {
  return this.add(a, c, true);
};
w.update = function(a, c) {
  return this.remove(a).add(a, c);
};
w.remove = function(a) {
  ba(a) && (a = ca(a, this.key));
  for (var c of this.index.values()) c.remove(a, true);
  if (this.reg.has(a)) {
    if (this.tag && !this.fastupdate) for (let b of this.tag.values()) for (let e of b) {
      c = e[0];
      const d = e[1], f = d.indexOf(a);
      f > -1 && (d.length > 1 ? d.splice(f, 1) : b.delete(c));
    }
    this.store && this.store.delete(a);
    this.reg.delete(a);
  }
  this.cache && this.cache.remove(a);
  return this;
};
w.clear = function() {
  const a = [];
  for (const c of this.index.values()) {
    const b = c.clear();
    b.then && a.push(b);
  }
  if (this.tag) for (const c of this.tag.values()) c.clear();
  this.store && this.store.clear();
  this.cache && this.cache.clear();
  return a.length ? Promise.all(a) : this;
};
w.contain = function(a) {
  return this.db ? this.index.get(this.field[0]).db.has(a) : this.reg.has(a);
};
w.cleanup = function() {
  for (const a of this.index.values()) a.cleanup();
  return this;
};
w.get = function(a) {
  return this.db ? this.index.get(this.field[0]).db.enrich(a).then(function(c) {
    return c[0] && c[0].doc || null;
  }) : this.store.get(a) || null;
};
w.set = function(a, c) {
  typeof a === "object" && (c = a, a = ca(c, this.key));
  this.store.set(a, c);
  return this;
};
w.searchCache = la;
w.export = jb;
w.import = kb;
Fa(Na.prototype);
function lb(a, c = 0) {
  let b = [], e = [];
  c && (c = 25e4 / c * 5e3 | 0);
  for (const d of a.entries()) e.push(d), e.length === c && (b.push(e), e = []);
  e.length && b.push(e);
  return b;
}
function mb(a, c) {
  c || (c = /* @__PURE__ */ new Map());
  for (let b = 0, e; b < a.length; b++) e = a[b], c.set(e[0], e[1]);
  return c;
}
function nb(a, c = 0) {
  let b = [], e = [];
  c && (c = 25e4 / c * 1e3 | 0);
  for (const d of a.entries()) e.push([d[0], lb(d[1])[0] || []]), e.length === c && (b.push(e), e = []);
  e.length && b.push(e);
  return b;
}
function ob(a, c) {
  c || (c = /* @__PURE__ */ new Map());
  for (let b = 0, e, d; b < a.length; b++) e = a[b], d = c.get(e[0]), c.set(e[0], mb(e[1], d));
  return c;
}
function pb(a) {
  let c = [], b = [];
  for (const e of a.keys()) b.push(e), b.length === 25e4 && (c.push(b), b = []);
  b.length && c.push(b);
  return c;
}
function qb(a, c) {
  c || (c = /* @__PURE__ */ new Set());
  for (let b = 0; b < a.length; b++) c.add(a[b]);
  return c;
}
function rb(a, c, b, e, d, f, g = 0) {
  const k = e && e.constructor === Array;
  var h = k ? e.shift() : e;
  if (!h) return this.export(a, c, d, f + 1);
  if ((h = a((c ? c + "." : "") + (g + 1) + "." + b, JSON.stringify(h))) && h.then) {
    const l = this;
    return h.then(function() {
      return rb.call(l, a, c, b, k ? e : null, d, f, g + 1);
    });
  }
  return rb.call(this, a, c, b, k ? e : null, d, f, g + 1);
}
function jb(a, c, b = 0, e = 0) {
  if (b < this.field.length) {
    const g = this.field[b];
    if ((c = this.index.get(g).export(a, g, b, e = 1)) && c.then) {
      const k = this;
      return c.then(function() {
        return k.export(a, g, b + 1);
      });
    }
    return this.export(a, g, b + 1);
  }
  let d, f;
  switch (e) {
    case 0:
      d = "reg";
      f = pb(this.reg);
      c = null;
      break;
    case 1:
      d = "tag";
      f = this.tag && nb(this.tag, this.reg.size);
      c = null;
      break;
    case 2:
      d = "doc";
      f = this.store && lb(this.store);
      c = null;
      break;
    default:
      return;
  }
  return rb.call(this, a, c, d, f || null, b, e);
}
function kb(a, c) {
  var b = a.split(".");
  b[b.length - 1] === "json" && b.pop();
  const e = b.length > 2 ? b[0] : "";
  b = b.length > 2 ? b[2] : b[1];
  if (this.worker && e) return this.index.get(e).import(a);
  if (c) {
    typeof c === "string" && (c = JSON.parse(c));
    if (e) return this.index.get(e).import(b, c);
    switch (b) {
      case "reg":
        this.fastupdate = false;
        this.reg = qb(c, this.reg);
        for (let d = 0, f; d < this.field.length; d++) f = this.index.get(this.field[d]), f.fastupdate = false, f.reg = this.reg;
        if (this.worker) {
          c = [];
          for (const d of this.index.values()) c.push(d.import(a));
          return Promise.all(c);
        }
        break;
      case "tag":
        this.tag = ob(c, this.tag);
        break;
      case "doc":
        this.store = mb(c, this.store);
    }
  }
}
function sb(a, c) {
  let b = "";
  for (const e of a.entries()) {
    a = e[0];
    const d = e[1];
    let f = "";
    for (let g = 0, k; g < d.length; g++) {
      k = d[g] || [""];
      let h = "";
      for (let l = 0; l < k.length; l++) h += (h ? "," : "") + (c === "string" ? '"' + k[l] + '"' : k[l]);
      h = "[" + h + "]";
      f += (f ? "," : "") + h;
    }
    f = '["' + a + '",[' + f + "]]";
    b += (b ? "," : "") + f;
  }
  return b;
}
T.prototype.remove = function(a, c) {
  const b = this.reg.size && (this.fastupdate ? this.reg.get(a) : this.reg.has(a));
  if (b) {
    if (this.fastupdate) for (let e = 0, d, f; e < b.length; e++) {
      if ((d = b[e]) && (f = d.length)) if (d[f - 1] === a) d.pop();
      else {
        const g = d.indexOf(a);
        g >= 0 && d.splice(g, 1);
      }
    }
    else tb(this.map, a), this.depth && tb(this.ctx, a);
    c || this.reg.delete(a);
  }
  this.db && (this.commit_task.push({ del: a }), this.M && ub(this));
  this.cache && this.cache.remove(a);
  return this;
};
function tb(a, c) {
  let b = 0;
  var e = typeof c === "undefined";
  if (a.constructor === Array) for (let d = 0, f, g, k; d < a.length; d++) {
    if ((f = a[d]) && f.length) {
      if (e) return 1;
      g = f.indexOf(c);
      if (g >= 0) {
        if (f.length > 1) return f.splice(g, 1), 1;
        delete a[d];
        if (b) return 1;
        k = 1;
      } else {
        if (k) return 1;
        b++;
      }
    }
  }
  else for (let d of a.entries()) e = d[0], tb(d[1], c) ? b++ : a.delete(e);
  return b;
}
var vb = { memory: { resolution: 1 }, performance: { resolution: 3, fastupdate: true, context: { depth: 1, resolution: 1 } }, match: { tokenize: "forward" }, score: { resolution: 9, context: { depth: 2, resolution: 3 } } };
T.prototype.add = function(a, c, b, e) {
  if (c && (a || a === 0)) {
    if (!e && !b && this.reg.has(a)) return this.update(a, c);
    e = this.depth;
    c = this.encoder.encode(c, !e);
    const l = c.length;
    if (l) {
      const m = I(), p = I(), u = this.resolution;
      for (let r = 0; r < l; r++) {
        let t = c[this.rtl ? l - 1 - r : r];
        var d = t.length;
        if (d && (e || !p[t])) {
          var f = this.score ? this.score(c, t, r, null, 0) : wb(u, l, r), g = "";
          switch (this.tokenize) {
            case "tolerant":
              Y(this, p, t, f, a, b);
              if (d > 2) {
                for (let n = 1, q, x, v, A; n < d - 1; n++) q = t.charAt(n), x = t.charAt(n + 1), v = t.substring(0, n) + x, A = t.substring(n + 2), g = v + q + A, Y(this, p, g, f, a, b), g = v + A, Y(this, p, g, f, a, b);
                Y(this, p, t.substring(0, t.length - 1), f, a, b);
              }
              break;
            case "full":
              if (d > 2) {
                for (let n = 0, q; n < d; n++) for (f = d; f > n; f--) {
                  g = t.substring(n, f);
                  q = this.rtl ? d - 1 - n : n;
                  var k = this.score ? this.score(c, t, r, g, q) : wb(u, l, r, d, q);
                  Y(this, p, g, k, a, b);
                }
                break;
              }
            case "bidirectional":
            case "reverse":
              if (d > 1) {
                for (k = d - 1; k > 0; k--) {
                  g = t[this.rtl ? d - 1 - k : k] + g;
                  var h = this.score ? this.score(c, t, r, g, k) : wb(u, l, r, d, k);
                  Y(this, p, g, h, a, b);
                }
                g = "";
              }
            case "forward":
              if (d > 1) {
                for (k = 0; k < d; k++) g += t[this.rtl ? d - 1 - k : k], Y(
                  this,
                  p,
                  g,
                  f,
                  a,
                  b
                );
                break;
              }
            default:
              if (Y(this, p, t, f, a, b), e && l > 1 && r < l - 1) for (d = this.N, g = t, f = Math.min(e + 1, this.rtl ? r + 1 : l - r), k = 1; k < f; k++) {
                t = c[this.rtl ? l - 1 - r - k : r + k];
                h = this.bidirectional && t > g;
                const n = this.score ? this.score(c, g, r, t, k - 1) : wb(d + (l / 2 > d ? 0 : 1), l, r, f - 1, k - 1);
                Y(this, m, h ? g : t, n, a, b, h ? t : g);
              }
          }
        }
      }
      this.fastupdate || this.reg.add(a);
    }
  }
  this.db && (this.commit_task.push(b ? { ins: a } : { del: a }), this.M && ub(this));
  return this;
};
function Y(a, c, b, e, d, f, g) {
  let k, h;
  if (!(k = c[b]) || g && !k[g]) {
    g ? (c = k || (c[b] = I()), c[g] = 1, h = a.ctx, (k = h.get(g)) ? h = k : h.set(g, h = a.keystore ? new R(a.keystore) : /* @__PURE__ */ new Map())) : (h = a.map, c[b] = 1);
    (k = h.get(b)) ? h = k : h.set(b, h = k = []);
    if (f) {
      for (let l = 0, m; l < k.length; l++) if ((m = k[l]) && m.includes(d)) {
        if (l <= e) return;
        m.splice(m.indexOf(d), 1);
        a.fastupdate && (c = a.reg.get(d)) && c.splice(c.indexOf(m), 1);
        break;
      }
    }
    h = h[e] || (h[e] = []);
    h.push(d);
    if (h.length === 2 ** 31 - 1) {
      c = new xa(h);
      if (a.fastupdate) for (let l of a.reg.values()) l.includes(h) && (l[l.indexOf(h)] = c);
      k[e] = h = c;
    }
    a.fastupdate && ((e = a.reg.get(d)) ? e.push(h) : a.reg.set(d, [h]));
  }
}
function wb(a, c, b, e, d) {
  return b && a > 1 ? c + (e || 0) <= a ? b + (d || 0) : (a - 1) / (c + (e || 0)) * (b + (d || 0)) + 1 | 0 : 0;
}
T.prototype.search = function(a, c, b) {
  b || (c || typeof a !== "object" ? typeof c === "object" && (b = c, c = 0) : (b = a, a = ""));
  if (b && b.cache) return b.cache = false, a = this.searchCache(a, c, b), b.cache = true, a;
  let e = [], d, f, g, k = 0, h, l, m, p, u;
  b && (a = b.query || a, c = b.limit || c, k = b.offset || 0, f = b.context, g = b.suggest, u = (h = b.resolve) && b.enrich, m = b.boost, p = b.resolution, l = this.db && b.tag);
  typeof h === "undefined" && (h = this.resolve);
  f = this.depth && f !== false;
  let r = this.encoder.encode(a, !f);
  d = r.length;
  c = c || (h ? 100 : 0);
  if (d === 1) return xb.call(
    this,
    r[0],
    "",
    c,
    k,
    h,
    u,
    l
  );
  if (d === 2 && f && !g) return xb.call(this, r[1], r[0], c, k, h, u, l);
  let t = I(), n = 0, q;
  f && (q = r[0], n = 1);
  p || p === 0 || (p = q ? this.N : this.resolution);
  if (this.db) {
    if (this.db.search && (b = this.db.search(this, r, c, k, g, h, u, l), b !== false)) return b;
    const x = this;
    return (async function() {
      for (let v, A; n < d; n++) {
        if ((A = r[n]) && !t[A]) {
          t[A] = 1;
          v = await yb(x, A, q, 0, 0, false, false);
          if (v = zb(v, e, g, p)) {
            e = v;
            break;
          }
          q && (g && v && e.length || (q = A));
        }
        g && q && n === d - 1 && !e.length && (p = x.resolution, q = "", n = -1, t = I());
      }
      return Ab(e, p, c, k, g, m, h);
    })();
  }
  for (let x, v; n < d; n++) {
    if ((v = r[n]) && !t[v]) {
      t[v] = 1;
      x = yb(this, v, q, 0, 0, false, false);
      if (x = zb(x, e, g, p)) {
        e = x;
        break;
      }
      q && (g && x && e.length || (q = v));
    }
    g && q && n === d - 1 && !e.length && (p = this.resolution, q = "", n = -1, t = I());
  }
  return Ab(e, p, c, k, g, m, h);
};
function Ab(a, c, b, e, d, f, g) {
  let k = a.length, h = a;
  if (k > 1) h = $a(a, c, b, e, d, f, g);
  else if (k === 1) return g ? Sa.call(null, a[0], b, e) : new X(a[0], this);
  return g ? h : new X(h, this);
}
function xb(a, c, b, e, d, f, g) {
  a = yb(this, a, c, b, e, d, f, g);
  return this.db ? a.then(function(k) {
    return d ? k || [] : new X(k, this);
  }) : a && a.length ? d ? Sa.call(this, a, b, e) : new X(a, this) : d ? [] : new X([], this);
}
function zb(a, c, b, e) {
  let d = [];
  if (a && a.length) {
    if (a.length <= e) {
      c.push(a);
      return;
    }
    for (let f = 0, g; f < e; f++) if (g = a[f]) d[f] = g;
    if (d.length) {
      c.push(d);
      return;
    }
  }
  if (!b) return d;
}
function yb(a, c, b, e, d, f, g, k) {
  let h;
  b && (h = a.bidirectional && c > b) && (h = b, b = c, c = h);
  if (a.db) return a.db.get(c, b, e, d, f, g, k);
  a = b ? (a = a.ctx.get(b)) && a.get(c) : a.map.get(c);
  return a;
}
function T(a, c) {
  if (!this || this.constructor !== T) return new T(a);
  if (a) {
    var b = M(a) ? a : a.preset;
    b && (a = Object.assign({}, vb[b], a));
  } else a = {};
  b = a.context;
  const e = b === true ? { depth: 1 } : b || {}, d = M(a.encoder) ? va[a.encoder] : a.encode || a.encoder || {};
  this.encoder = d.encode ? d : typeof d === "object" ? new ka(d) : { encode: d };
  this.resolution = a.resolution || 9;
  this.tokenize = b = (b = a.tokenize) && b !== "default" && b !== "exact" && b || "strict";
  this.depth = b === "strict" && e.depth || 0;
  this.bidirectional = e.bidirectional !== false;
  this.fastupdate = !!a.fastupdate;
  this.score = a.score || null;
  (b = a.keystore || 0) && (this.keystore = b);
  this.map = b ? new R(b) : /* @__PURE__ */ new Map();
  this.ctx = b ? new R(b) : /* @__PURE__ */ new Map();
  this.reg = c || (this.fastupdate ? b ? new R(b) : /* @__PURE__ */ new Map() : b ? new S(b) : /* @__PURE__ */ new Set());
  this.N = e.resolution || 3;
  this.rtl = d.rtl || a.rtl || false;
  this.cache = (b = a.cache || null) && new ma(b);
  this.resolve = a.resolve !== false;
  if (b = a.db) this.db = this.mount(b);
  this.M = a.commit !== false;
  this.commit_task = [];
  this.commit_timer = null;
  this.priority = a.priority || 4;
}
w = T.prototype;
w.mount = function(a) {
  this.commit_timer && (clearTimeout(this.commit_timer), this.commit_timer = null);
  return a.mount(this);
};
w.commit = function() {
  this.commit_timer && (clearTimeout(this.commit_timer), this.commit_timer = null);
  return this.db.commit(this);
};
w.destroy = function() {
  this.commit_timer && (clearTimeout(this.commit_timer), this.commit_timer = null);
  return this.db.destroy();
};
function ub(a) {
  a.commit_timer || (a.commit_timer = setTimeout(function() {
    a.commit_timer = null;
    a.db.commit(a);
  }, 1));
}
w.clear = function() {
  this.map.clear();
  this.ctx.clear();
  this.reg.clear();
  this.cache && this.cache.clear();
  return this.db ? (this.commit_timer && clearTimeout(this.commit_timer), this.commit_timer = null, this.commit_task = [], this.db.clear()) : this;
};
w.append = function(a, c) {
  return this.add(a, c, true);
};
w.contain = function(a) {
  return this.db ? this.db.has(a) : this.reg.has(a);
};
w.update = function(a, c) {
  const b = this, e = this.remove(a);
  return e && e.then ? e.then(() => b.add(a, c)) : this.add(a, c);
};
w.cleanup = function() {
  if (!this.fastupdate) return this;
  tb(this.map);
  this.depth && tb(this.ctx);
  return this;
};
w.searchCache = la;
w.export = function(a, c, b = 0, e = 0) {
  let d, f;
  switch (e) {
    case 0:
      d = "reg";
      f = pb(this.reg);
      break;
    case 1:
      d = "cfg";
      f = null;
      break;
    case 2:
      d = "map";
      f = lb(this.map, this.reg.size);
      break;
    case 3:
      d = "ctx";
      f = nb(this.ctx, this.reg.size);
      break;
    default:
      return;
  }
  return rb.call(this, a, c, d, f, b, e);
};
w.import = function(a, c) {
  if (c) switch (typeof c === "string" && (c = JSON.parse(c)), a = a.split("."), a[a.length - 1] === "json" && a.pop(), a.length === 3 && a.shift(), a = a.length > 1 ? a[1] : a[0], a) {
    case "reg":
      this.fastupdate = false;
      this.reg = qb(c, this.reg);
      break;
    case "map":
      this.map = mb(c, this.map);
      break;
    case "ctx":
      this.ctx = ob(c, this.ctx);
  }
};
w.serialize = function(a = true) {
  let c = "", b = "", e = "";
  if (this.reg.size) {
    let f;
    for (var d of this.reg.keys()) f || (f = typeof d), c += (c ? "," : "") + (f === "string" ? '"' + d + '"' : d);
    c = "index.reg=new Set([" + c + "]);";
    b = sb(this.map, f);
    b = "index.map=new Map([" + b + "]);";
    for (const g of this.ctx.entries()) {
      d = g[0];
      let k = sb(g[1], f);
      k = "new Map([" + k + "])";
      k = '["' + d + '",' + k + "]";
      e += (e ? "," : "") + k;
    }
    e = "index.ctx=new Map([" + e + "]);";
  }
  return a ? "function inject(index){" + c + b + e + "}" : c + b + e;
};
Fa(T.prototype);
var Bb = typeof window !== "undefined" && (window.indexedDB || window.mozIndexedDB || window.webkitIndexedDB || window.msIndexedDB);
var Cb = ["map", "ctx", "tag", "reg", "cfg"];
var Db = I();
function Eb(a, c = {}) {
  if (!this || this.constructor !== Eb) return new Eb(a, c);
  typeof a === "object" && (c = a, a = a.name);
  a || console.info("Default storage space was used, because a name was not passed.");
  this.id = "flexsearch" + (a ? ":" + a.toLowerCase().replace(/[^a-z0-9_\-]/g, "") : "");
  this.field = c.field ? c.field.toLowerCase().replace(/[^a-z0-9_\-]/g, "") : "";
  this.type = c.type;
  this.fastupdate = this.support_tag_search = false;
  this.db = null;
  this.h = {};
}
w = Eb.prototype;
w.mount = function(a) {
  if (a.index) return a.mount(this);
  a.db = this;
  return this.open();
};
w.open = function() {
  if (this.db) return this.db;
  let a = this;
  navigator.storage && navigator.storage.persist && navigator.storage.persist();
  Db[a.id] || (Db[a.id] = []);
  Db[a.id].push(a.field);
  const c = Bb.open(a.id, 1);
  c.onupgradeneeded = function() {
    const b = a.db = this.result;
    for (let e = 0, d; e < Cb.length; e++) {
      d = Cb[e];
      for (let f = 0, g; f < Db[a.id].length; f++) g = Db[a.id][f], b.objectStoreNames.contains(d + (d !== "reg" ? g ? ":" + g : "" : "")) || b.createObjectStore(d + (d !== "reg" ? g ? ":" + g : "" : ""));
    }
  };
  return a.db = Z(c, function(b) {
    a.db = b;
    a.db.onversionchange = function() {
      a.close();
    };
  });
};
w.close = function() {
  this.db && this.db.close();
  this.db = null;
};
w.destroy = function() {
  const a = Bb.deleteDatabase(this.id);
  return Z(a);
};
w.clear = function() {
  const a = [];
  for (let b = 0, e; b < Cb.length; b++) {
    e = Cb[b];
    for (let d = 0, f; d < Db[this.id].length; d++) f = Db[this.id][d], a.push(e + (e !== "reg" ? f ? ":" + f : "" : ""));
  }
  const c = this.db.transaction(a, "readwrite");
  for (let b = 0; b < a.length; b++) c.objectStore(a[b]).clear();
  return Z(c);
};
w.get = function(a, c, b = 0, e = 0, d = true, f = false) {
  a = this.db.transaction((c ? "ctx" : "map") + (this.field ? ":" + this.field : ""), "readonly").objectStore((c ? "ctx" : "map") + (this.field ? ":" + this.field : "")).get(c ? c + ":" + a : a);
  const g = this;
  return Z(a).then(function(k) {
    let h = [];
    if (!k || !k.length) return h;
    if (d) {
      if (!b && !e && k.length === 1) return k[0];
      for (let l = 0, m; l < k.length; l++) if ((m = k[l]) && m.length) {
        if (e >= m.length) {
          e -= m.length;
          continue;
        }
        const p = b ? e + Math.min(m.length - e, b) : m.length;
        for (let u = e; u < p; u++) h.push(m[u]);
        e = 0;
        if (h.length === b) break;
      }
      return f ? g.enrich(h) : h;
    }
    return k;
  });
};
w.tag = function(a, c = 0, b = 0, e = false) {
  a = this.db.transaction("tag" + (this.field ? ":" + this.field : ""), "readonly").objectStore("tag" + (this.field ? ":" + this.field : "")).get(a);
  const d = this;
  return Z(a).then(function(f) {
    if (!f || !f.length || b >= f.length) return [];
    if (!c && !b) return f;
    f = f.slice(b, b + c);
    return e ? d.enrich(f) : f;
  });
};
w.enrich = function(a) {
  typeof a !== "object" && (a = [a]);
  const c = this.db.transaction("reg", "readonly").objectStore("reg"), b = [];
  for (let e = 0; e < a.length; e++) b[e] = Z(c.get(a[e]));
  return Promise.all(b).then(function(e) {
    for (let d = 0; d < e.length; d++) e[d] = { id: a[d], doc: e[d] ? JSON.parse(e[d]) : null };
    return e;
  });
};
w.has = function(a) {
  a = this.db.transaction("reg", "readonly").objectStore("reg").getKey(a);
  return Z(a).then(function(c) {
    return !!c;
  });
};
w.search = null;
w.info = function() {
};
w.transaction = function(a, c, b) {
  a += a !== "reg" ? this.field ? ":" + this.field : "" : "";
  let e = this.h[a + ":" + c];
  if (e) return b.call(this, e);
  let d = this.db.transaction(a, c);
  this.h[a + ":" + c] = e = d.objectStore(a);
  const f = b.call(this, e);
  this.h[a + ":" + c] = null;
  return Z(d).finally(function() {
    return f;
  });
};
w.commit = async function(a) {
  let c = a.commit_task, b = [];
  a.commit_task = [];
  for (let e = 0, d; e < c.length; e++) d = c[e], d.del && b.push(d.del);
  b.length && await this.remove(b);
  a.reg.size && (await this.transaction("map", "readwrite", function(e) {
    for (const d of a.map) {
      const f = d[0], g = d[1];
      g.length && (e.get(f).onsuccess = function() {
        let k = this.result;
        var h;
        if (k && k.length) {
          const l = Math.max(k.length, g.length);
          for (let m = 0, p, u; m < l; m++) if ((u = g[m]) && u.length) {
            if ((p = k[m]) && p.length) for (h = 0; h < u.length; h++) p.push(u[h]);
            else k[m] = u;
            h = 1;
          }
        } else k = g, h = 1;
        h && e.put(k, f);
      });
    }
  }), await this.transaction("ctx", "readwrite", function(e) {
    for (const d of a.ctx) {
      const f = d[0], g = d[1];
      for (const k of g) {
        const h = k[0], l = k[1];
        l.length && (e.get(f + ":" + h).onsuccess = function() {
          let m = this.result;
          var p;
          if (m && m.length) {
            const u = Math.max(m.length, l.length);
            for (let r = 0, t, n; r < u; r++) if ((n = l[r]) && n.length) {
              if ((t = m[r]) && t.length) for (p = 0; p < n.length; p++) t.push(n[p]);
              else m[r] = n;
              p = 1;
            }
          } else m = l, p = 1;
          p && e.put(m, f + ":" + h);
        });
      }
    }
  }), a.store ? await this.transaction(
    "reg",
    "readwrite",
    function(e) {
      for (const d of a.store) {
        const f = d[0], g = d[1];
        e.put(typeof g === "object" ? JSON.stringify(g) : 1, f);
      }
    }
  ) : a.bypass || await this.transaction("reg", "readwrite", function(e) {
    for (const d of a.reg.keys()) e.put(1, d);
  }), a.tag && await this.transaction("tag", "readwrite", function(e) {
    for (const d of a.tag) {
      const f = d[0], g = d[1];
      g.length && (e.get(f).onsuccess = function() {
        let k = this.result;
        k = k && k.length ? k.concat(g) : g;
        e.put(k, f);
      });
    }
  }), a.map.clear(), a.ctx.clear(), a.tag && a.tag.clear(), a.store && a.store.clear(), a.document || a.reg.clear());
};
function Fb(a, c, b) {
  const e = a.value;
  let d, f = 0;
  for (let g = 0, k; g < e.length; g++) {
    if (k = b ? e : e[g]) {
      for (let h = 0, l, m; h < c.length; h++) if (m = c[h], l = k.indexOf(m), l >= 0) if (d = 1, k.length > 1) k.splice(l, 1);
      else {
        e[g] = [];
        break;
      }
      f += k.length;
    }
    if (b) break;
  }
  f ? d && a.update(e) : a.delete();
  a.continue();
}
w.remove = function(a) {
  typeof a !== "object" && (a = [a]);
  return Promise.all([this.transaction("map", "readwrite", function(c) {
    c.openCursor().onsuccess = function() {
      const b = this.result;
      b && Fb(b, a);
    };
  }), this.transaction("ctx", "readwrite", function(c) {
    c.openCursor().onsuccess = function() {
      const b = this.result;
      b && Fb(b, a);
    };
  }), this.transaction("tag", "readwrite", function(c) {
    c.openCursor().onsuccess = function() {
      const b = this.result;
      b && Fb(b, a, true);
    };
  }), this.transaction("reg", "readwrite", function(c) {
    for (let b = 0; b < a.length; b++) c.delete(a[b]);
  })]);
};
function Z(a, c) {
  return new Promise((b, e) => {
    a.onsuccess = a.oncomplete = function() {
      c && c(this.result);
      c = null;
      b(this.result);
    };
    a.onerror = a.onblocked = e;
    a = null;
  });
}
var Document = Na;

// src/store/search.ts
function flattenToText(value, depth = 0) {
  if (depth > 12) return "";
  if (value === null || value === void 0) return "";
  if (typeof value === "string") return value;
  if (typeof value === "number" || typeof value === "boolean") return String(value);
  if (Array.isArray(value)) {
    return value.map((v) => flattenToText(v, depth + 1)).join(" ");
  }
  if (typeof value === "object") {
    return Object.entries(value).map(([k, v]) => k + " " + flattenToText(v, depth + 1)).join(" ");
  }
  return "";
}
var SearchIndex = class extends Service()(
  "@tmnl/rlm/SearchIndex"
) {
};
var SearchIndexLive = effect(
  SearchIndex,
  gen2(function* () {
    const sql = yield* SqlClient;
    const flexIndex = new Document({
      document: {
        id: "id",
        index: [
          { field: "summary", tokenize: "forward" },
          { field: "tags", tokenize: "forward" },
          { field: "content", tokenize: "forward" }
        ]
      }
    });
    const countRef = yield* make20(0);
    const readyRef = yield* make20(false);
    const compositeId = (ns, key) => `${ns}::${key}`;
    const toSearchDoc = (ns, key, data, tags) => {
      const meta = data._meta;
      return {
        id: compositeId(ns, key),
        summary: meta?.summary ?? "",
        tags: tags.join(" "),
        content: flattenToText(data)
      };
    };
    const loadAll = gen2(function* () {
      const rows = yield* sql`
        SELECT collection, key, data, tags FROM objects ORDER BY collection, key
      `;
      let count = 0;
      for (const row of rows) {
        const data = JSON.parse(row.data);
        const tags = JSON.parse(row.tags ?? "[]");
        const doc = toSearchDoc(row.collection, row.key, data, tags);
        flexIndex.add(doc);
        count++;
      }
      yield* set3(countRef, count);
      yield* set3(readyRef, true);
      return count;
    });
    const indexed = yield* loadAll.pipe(
      catch_2(() => succeed5(0))
    );
    const searchFlex = (text, nsGlob) => (
      // @ts-expect-error SqlError in error channel — fixed in Effect rewrite
      gen2(function* () {
        const isReady = yield* get2(readyRef);
        if (!isReady) return [];
        const results = flexIndex.search(text, { limit: 100, enrich: true });
        const hitMap = /* @__PURE__ */ new Map();
        for (const fieldResult of results) {
          const field = fieldResult.field;
          const items = fieldResult.result;
          for (let i = 0; i < items.length; i++) {
            const id = typeof items[i] === "object" ? items[i].id : items[i];
            const positionScore = 1 - i / Math.max(items.length, 1);
            const fieldWeight = field === "summary" ? 3 : field === "tags" ? 2 : 1;
            const score = positionScore * fieldWeight;
            const existing = hitMap.get(id);
            if (existing) {
              existing.score = Math.max(existing.score, score);
              if (!existing.fields.includes(field)) existing.fields.push(field);
            } else {
              hitMap.set(id, { score, fields: [field] });
            }
          }
        }
        if (hitMap.size === 0) return [];
        const ids = Array.from(hitMap.keys());
        const hits = [];
        for (const id of ids) {
          const [ns, key] = id.split("::");
          if (!ns || !key) continue;
          if (nsGlob && !namespaceMatchesGlob(ns, nsGlob)) continue;
          const rows = yield* sql`
            SELECT collection, key, summary, source, intent, tags, created_at, updated_at
            FROM objects WHERE collection = ${ns} AND key = ${key}
          `;
          if (rows.length === 0) continue;
          const r = rows[0];
          const match6 = hitMap.get(id);
          hits.push({
            collection: r.collection,
            key: r.key,
            summary: r.summary ?? "",
            source: r.source,
            intent: r.intent,
            tags: JSON.parse(r.tags ?? "[]"),
            created_at: r.created_at,
            updated_at: r.updated_at,
            score: match6.score,
            matchedFields: match6.fields
          });
        }
        hits.sort((a, b) => b.score - a.score);
        return hits;
      }).pipe(withSpan2("SearchIndex.searchFlex", { attributes: { text, nsGlob } }))
    );
    const searchFts5 = (text, nsGlob) => (
      // @ts-expect-error SqlError in error channel — fixed in Effect rewrite
      gen2(function* () {
        const rows = yield* sql.unsafe(
          `SELECT o.collection, o.key, o.summary, o.source, o.intent,
                  o.tags, o.created_at, o.updated_at, rank
           FROM objects_fts fts
           JOIN objects o ON o.rowid = fts.rowid
           WHERE objects_fts MATCH ?
           ORDER BY rank
           LIMIT 50`,
          [text]
        );
        const entries = rows.map((r) => ({
          collection: r.collection,
          key: r.key,
          summary: r.summary ?? "",
          source: r.source,
          intent: r.intent,
          tags: JSON.parse(r.tags ?? "[]"),
          created_at: r.created_at,
          updated_at: r.updated_at,
          score: Math.abs(r.rank),
          // FTS5 rank is negative
          matchedFields: ["fts5"]
        }));
        if (nsGlob) {
          return entries.filter((e) => namespaceMatchesGlob(e.collection, nsGlob));
        }
        return entries;
      }).pipe(withSpan2("SearchIndex.searchFts5", { attributes: { text, nsGlob } }))
    );
    return SearchIndex.of({
      search: (text, nsGlob) => gen2(function* () {
        const isReady = yield* get2(readyRef);
        if (isReady) {
          const results = yield* searchFlex(text, nsGlob);
          if (results.length > 0) return results;
          return yield* searchFts5(text, nsGlob).pipe(
            catch_2(() => succeed5([]))
          );
        }
        return yield* searchFts5(text, nsGlob).pipe(
          catch_2(() => succeed5([]))
        );
      }).pipe(withSpan2("SearchIndex.search", { attributes: { text, nsGlob } })),
      notify: (ns, key, data, tags) => sync2(() => {
        const doc = toSearchDoc(ns, key, data, tags);
        try {
          flexIndex.remove(doc.id);
        } catch {
        }
        flexIndex.add(doc);
      }).pipe(
        tap2(() => update(countRef, (n) => n + 1))
      ),
      notifyRemove: (ns, key) => sync2(() => {
        try {
          flexIndex.remove(compositeId(ns, key));
        } catch {
        }
      }).pipe(
        tap2(() => update(countRef, (n) => Math.max(0, n - 1)))
      ),
      rebuild: () => gen2(function* () {
        const rows = yield* sql`SELECT collection, key FROM objects`;
        for (const row of rows) {
          try {
            flexIndex.remove(compositeId(row.collection, row.key));
          } catch {
          }
        }
        const count = yield* loadAll;
        yield* sql.unsafe(`INSERT INTO objects_fts(objects_fts) VALUES('rebuild')`);
        return { indexed: count };
      }).pipe(withSpan2("SearchIndex.rebuild")),
      stats: () => gen2(function* () {
        const count = yield* get2(countRef);
        const ready = yield* get2(readyRef);
        return { flexCount: count, ready };
      })
    });
  })
);

// src/store/domains.ts
var DomainRegistry = class extends Service()(
  "@tmnl/rlm/DomainRegistry"
) {
};
var DomainRegistryLive = effect(
  DomainRegistry,
  gen2(function* () {
    const store = yield* RlmStore;
    return DomainRegistry.of({
      register: (name, config) => gen2(function* () {
        const validConfig = validateDomainConfig(config);
        yield* store.put("_system.domains", name, {
          _meta: {
            summary: `Domain config: ${name}`,
            schema: "domain-config-v1"
          },
          ...validConfig
        });
      }).pipe(withSpan2("DomainRegistry.register", { attributes: { name } })),
      list: () => gen2(function* () {
        const objects = yield* store.query("_system.domains");
        return objects.map((obj) => {
          const { _meta, ...config } = obj.data;
          return {
            name: obj.key,
            config
          };
        });
      }).pipe(withSpan2("DomainRegistry.list")),
      getConfig: (ns) => gen2(function* () {
        const segments = ns.split(".");
        for (let i = segments.length; i > 0; i--) {
          const candidate = segments.slice(0, i).join(".");
          const data = yield* store.get("_system.domains", candidate);
          if (data) return data;
        }
        return null;
      }).pipe(withSpan2("DomainRegistry.getConfig", { attributes: { ns } }))
    });
  })
);

// src/store/export.ts
var Address = String4.pipe(
  check(makeFilter2(
    (s) => /^[a-z_][a-z0-9._-]*\/[a-z][a-z0-9-]*(--\d{8}T\d{6})?$/.test(s) ? void 0 : `Invalid address "${s}". Must be "collection/key" (e.g. "effect.api/filesystem-v4")`
  )),
  brand2("Address")
);
function parseAddress(addr) {
  const idx = addr.indexOf("/");
  if (idx <= 0 || idx === addr.length - 1) return null;
  return { collection: addr.slice(0, idx), key: addr.slice(idx + 1) };
}
function buildAddress(collection, key) {
  return `${collection}/${key}`;
}
var KeyGlob = String4.pipe(
  check(makeFilter2(
    (s) => s.length > 0 && /^[a-z0-9*_-]+$/.test(s) ? void 0 : `Invalid key glob "${s}". Must be lowercase alphanum/dash with optional * wildcard`
  )),
  brand2("KeyGlob")
);
function keyMatchesGlob(key, glob) {
  if (glob === "*") return true;
  if (glob.endsWith("*") && !glob.startsWith("*")) {
    return key.startsWith(glob.slice(0, -1));
  }
  if (glob.startsWith("*") && !glob.endsWith("*")) {
    return key.endsWith(glob.slice(1));
  }
  if (glob.startsWith("*") && glob.endsWith("*") && glob.length > 2) {
    return key.includes(glob.slice(1, -1));
  }
  return key === glob;
}
var ExportFormat = Literals(["json", "sqlite", "procedures"]);
var ImportMode = Literals(["merge", "replace"]);
var ProfileName = String4.pipe(
  check(makeFilter2(
    (s) => /^[a-z][a-z0-9-]*$/.test(s) ? void 0 : `Invalid profile name "${s}". Must be lowercase kebab-case (e.g. "effect-knowledge")`
  )),
  brand2("ProfileName")
);
var ExportOptions = Struct({
  path: String4,
  glob: optional(String4),
  format: optional(ExportFormat),
  pretty: optional(Boolean2),
  keys: optional(ArraySchema(String4)),
  keyGlob: optional(String4),
  /** Name this export — embedded in manifest, used as default profile name on import */
  profile: optional(String4),
  /** Tool guide manifest entry — embedded in manifest, describes what this export provides */
  manifest: optional(String4),
  /** Export only objects from this applied profile */
  fromProfile: optional(String4),
  /** Export objects added/changed after this profile was applied */
  since: optional(String4)
});
var ImportOptions = Struct({
  path: String4,
  mode: optional(ImportMode),
  glob: optional(String4),
  keys: optional(ArraySchema(String4)),
  keyGlob: optional(String4),
  /** Profile name for this import. Falls back to manifest.profile, then anonymous. */
  profile: optional(String4),
  /** Tool guide manifest entry — MANDATORY for named profiles.
   *  What this profile contributes to the system's capabilities.
   *  Example: "Effect v4 API patterns, gotchas, and schema reference" */
  manifest: optional(String4)
});
var ExportedObject = Struct({
  collection: String4,
  key: String4,
  data: Unknown2,
  tags: ArraySchema(String4)
});
var ExportManifest = Struct({
  version: Literal2(1),
  format: Literal2("rlm-json"),
  exportedAt: String4,
  glob: NullOr(String4),
  /** Profile name — embedded at export time, used as default on import */
  profile: optional(NullOr(String4)),
  /** Tool guide manifest entry — travels with the profile */
  manifest: optional(NullOr(String4)),
  collections: ArraySchema(String4),
  objectCount: Number5,
  objects: ArraySchema(ExportedObject)
});
var ImportResult = Struct({
  mode: ImportMode,
  profile: NullOr(String4),
  collectionsAffected: ArraySchema(String4),
  objectsImported: Number5,
  objectsSkipped: Number5,
  collectionsCleared: Number5
});
var ProfileRecord = Struct({
  name: String4,
  /** Tool guide manifest entry — MANDATORY. What this profile contributes. */
  manifest: String4,
  appliedAt: String4,
  sourcePath: String4,
  mode: ImportMode,
  objectCount: Number5,
  /** Addresses of every object this profile imported */
  objects: ArraySchema(String4),
  collectionsAffected: ArraySchema(String4)
});
var ProfileSummary = Struct({
  name: String4,
  manifest: String4,
  appliedAt: String4,
  objectCount: Number5,
  collectionsAffected: ArraySchema(String4)
});
function matchesKeyFilter(collection, key, filter5) {
  const hasKeys = filter5.keys && filter5.keys.length > 0;
  const hasKeyGlob = !!filter5.keyGlob;
  if (!hasKeys && !hasKeyGlob) return true;
  if (hasKeys) {
    const addr = buildAddress(collection, key);
    if (filter5.keys.includes(addr)) return true;
  }
  if (hasKeyGlob) {
    if (keyMatchesGlob(key, filter5.keyGlob)) return true;
  }
  return false;
}
var PROFILES_NS = "_system.profiles";
var ExportService = class extends Service()(
  "@tmnl/rlm/ExportService"
) {
};
var ExportServiceLive = effect(
  ExportService,
  gen2(function* () {
    const store = yield* RlmStore;
    const fs = yield* FileSystem;
    const getProfileRecord = (name) => gen2(function* () {
      const items = yield* store.query(PROFILES_NS);
      return items.find((i) => i.key === name) ?? null;
    });
    const objectHasProfile = (data, profileName) => {
      if (typeof data !== "object" || data === null) return false;
      const meta = data._meta;
      if (typeof meta !== "object" || meta === null) return false;
      return meta.profile === profileName;
    };
    return ExportService.of({
      // ── Export ──────────────────────────────────────────────
      exportStore: (opts) => gen2(function* () {
        const format2 = opts.format ?? "json";
        const glob = opts.glob ?? null;
        const profileName = opts.profile ?? null;
        if (format2 === "sqlite") {
          return yield* fail5(new Error(
            'SQLite export is handled at the API layer (needs db path). Use format: "json" or "procedures".'
          ));
        }
        const effectiveGlob = format2 === "procedures" ? "_system.procedures" : glob;
        let fromProfileAddresses = null;
        if (opts.fromProfile) {
          const rec = yield* getProfileRecord(opts.fromProfile);
          if (!rec) {
            return yield* fail5(new Error(`Profile "${opts.fromProfile}" not found`));
          }
          const data = rec.data;
          fromProfileAddresses = new Set(data.objects);
        }
        let sinceTimestamp = null;
        if (opts.since) {
          const rec = yield* getProfileRecord(opts.since);
          if (!rec) {
            return yield* fail5(new Error(`Profile "${opts.since}" not found`));
          }
          sinceTimestamp = rec.data.appliedAt;
        }
        let targetCollectionNames;
        if (fromProfileAddresses) {
          const colSet = /* @__PURE__ */ new Set();
          for (const addr of fromProfileAddresses) {
            const parsed = parseAddress(addr);
            if (parsed) colSet.add(parsed.collection);
          }
          targetCollectionNames = [...colSet];
        } else if (opts.keys && opts.keys.length > 0 && !effectiveGlob) {
          const colSet = /* @__PURE__ */ new Set();
          for (const addr of opts.keys) {
            const parsed = parseAddress(addr);
            if (parsed) colSet.add(parsed.collection);
          }
          targetCollectionNames = [...colSet];
        } else {
          const allCollections = yield* store.collections();
          const filtered = effectiveGlob ? allCollections.filter((c) => namespaceMatchesGlob(c.name, effectiveGlob)) : allCollections;
          targetCollectionNames = filtered.map((c) => c.name);
        }
        const objects = [];
        for (const colName of targetCollectionNames) {
          const items = yield* store.query(colName);
          for (const item of items) {
            if (!matchesKeyFilter(item.collection, item.key, opts)) continue;
            if (fromProfileAddresses) {
              const addr = buildAddress(item.collection, item.key);
              if (!fromProfileAddresses.has(addr)) continue;
            }
            if (sinceTimestamp && item.updated_at) {
              const itemTime = item.updated_at.replace(" ", "T") + (item.updated_at.includes("T") ? "" : "Z");
              const sinceTime = sinceTimestamp.replace(" ", "T");
              if (itemTime <= sinceTime) continue;
            }
            objects.push({
              collection: item.collection,
              key: item.key,
              data: item.data,
              tags: item.tags
            });
          }
        }
        const manifest = {
          version: 1,
          format: "rlm-json",
          exportedAt: (/* @__PURE__ */ new Date()).toISOString(),
          glob: effectiveGlob,
          profile: profileName,
          manifest: opts.manifest ?? null,
          collections: [...new Set(objects.map((o) => o.collection))],
          objectCount: objects.length,
          objects
        };
        const json = opts.pretty !== false ? JSON.stringify(manifest, null, 2) : JSON.stringify(manifest);
        yield* fs.writeFileString(opts.path, json);
        return manifest;
      }),
      // ── Import ─────────────────────────────────────────────
      importStore: (opts) => gen2(function* () {
        const mode = opts.mode ?? "merge";
        const content = yield* fs.readFileString(opts.path);
        const raw = JSON.parse(content);
        const manifest = decodeUnknownSync(ExportManifest)(raw);
        const profileName = opts.profile ?? manifest.profile ?? null;
        const profileManifest = opts.manifest ?? manifest.manifest ?? (profileName ? `[${profileName}] ${manifest.objectCount} objects from ${manifest.collections.join(", ")}` : null);
        const filteredObjects = manifest.objects.filter((obj) => {
          if (opts.glob && !namespaceMatchesGlob(obj.collection, opts.glob)) return false;
          return matchesKeyFilter(obj.collection, obj.key, opts);
        });
        const affectedCollections = /* @__PURE__ */ new Set();
        for (const obj of filteredObjects) {
          affectedCollections.add(obj.collection);
        }
        let imported = 0;
        let skipped = manifest.objects.length - filteredObjects.length;
        let cleared = 0;
        if (mode === "replace") {
          for (const colName of affectedCollections) {
            yield* store.clear(colName);
            cleared++;
          }
        }
        const importedAddresses = [];
        for (const obj of filteredObjects) {
          try {
            let data = obj.data;
            if (profileName) {
              const existingMeta = typeof data._meta === "object" && data._meta !== null ? data._meta : {};
              const summary = existingMeta.summary || `[profile:${profileName}] ${obj.collection}/${obj.key}`;
              data = {
                ...data,
                _meta: { ...existingMeta, summary, profile: profileName }
              };
            }
            yield* store.put(
              obj.collection,
              obj.key,
              data,
              obj.tags.length > 0 ? { tags: obj.tags } : void 0
            );
            importedAddresses.push(buildAddress(obj.collection, obj.key));
            imported++;
          } catch {
            skipped++;
          }
        }
        if (profileName && imported > 0) {
          const record2 = {
            name: profileName,
            manifest: profileManifest,
            appliedAt: (/* @__PURE__ */ new Date()).toISOString(),
            sourcePath: opts.path,
            mode,
            objectCount: imported,
            objects: importedAddresses,
            collectionsAffected: [...affectedCollections]
          };
          const ledgerData = {
            ...record2,
            _meta: {
              summary: `[profile] ${profileName} \u2014 ${imported} objects from ${opts.path}`,
              source: "export-service",
              type: "profile"
            }
          };
          yield* store.put(PROFILES_NS, profileName, ledgerData, {
            tags: ["profile", profileName]
          });
        }
        return {
          mode,
          profile: profileName,
          collectionsAffected: [...affectedCollections],
          objectsImported: imported,
          objectsSkipped: skipped,
          collectionsCleared: cleared
        };
      }),
      // ── List Profiles ──────────────────────────────────────
      listProfiles: () => gen2(function* () {
        const items = yield* store.query(PROFILES_NS);
        return items.map((item) => {
          const data = item.data;
          return {
            name: data.name,
            manifest: data.manifest ?? `[${data.name}] ${data.objectCount} objects`,
            appliedAt: data.appliedAt,
            objectCount: data.objectCount,
            collectionsAffected: data.collectionsAffected
          };
        });
      }),
      // ── Remove Profile ─────────────────────────────────────
      removeProfile: (name) => gen2(function* () {
        const rec = yield* getProfileRecord(name);
        if (!rec) {
          return yield* fail5(new Error(`Profile "${name}" not found`));
        }
        const data = rec.data;
        let removed = 0;
        const touchedCollections = /* @__PURE__ */ new Set();
        for (const addr of data.objects) {
          const parsed = parseAddress(addr);
          if (!parsed) continue;
          const current = yield* store.getRaw(parsed.collection, parsed.key);
          if (current && objectHasProfile(current, name)) {
            yield* store.del(parsed.collection, parsed.key);
            touchedCollections.add(parsed.collection);
            removed++;
          }
        }
        yield* store.del(PROFILES_NS, name);
        return { removed, collections: [...touchedCollections] };
      })
    });
  })
);

// src/store/builders.ts
var QueryBuilder = class {
  store;
  searchIndex;
  run;
  state;
  constructor(store, searchIndex, run2, ns) {
    this.store = store;
    this.searchIndex = searchIndex;
    this.run = run2;
    this.state = { ns, tagFilters: [] };
  }
  /** Filter by tags (AND logic) */
  tagged(...tags) {
    this.state.tagFilters.push(...tags);
    return this;
  }
  /** Filter by JSON path value */
  where(path, value) {
    this.state.jsonPath = path;
    this.state.jsonValue = value;
    return this;
  }
  /** Full-text search filter */
  search(text) {
    this.state.searchText = text;
    return this;
  }
  /** Limit results */
  limit(n) {
    this.state.maxResults = n;
    return this;
  }
  // ── Terminals ────────────────────────────────────────────────
  /** Get just the keys */
  async keys() {
    const entries = await this._resolve();
    return entries.map((e) => e.key);
  }
  /** Get full objects (data only, no _meta) */
  async entries() {
    return this._resolveObjects();
  }
  /** Get catalog summaries */
  async summaries() {
    return this._resolve();
  }
  /** Count matching entries */
  async count() {
    const entries = await this._resolve();
    return entries.length;
  }
  // ── Internal ─────────────────────────────────────────────────
  async _resolve() {
    const { ns, searchText, tagFilters } = this.state;
    if (searchText) {
      let results2 = await this.run(
        this.searchIndex.search(searchText, ns)
      );
      if (tagFilters.length > 0) {
        results2 = results2.filter(
          (r) => tagFilters.every((t) => r.tags.includes(t))
        );
      }
      if (this.state.maxResults) {
        results2 = results2.slice(0, this.state.maxResults);
      }
      return results2;
    }
    let results = await this.run(
      this.store.catalog(ns + "*")
    );
    results = results.filter(
      (r) => r.collection === ns || r.collection.startsWith(ns + ".")
    );
    if (tagFilters.length > 0) {
      results = results.filter(
        (r) => tagFilters.every((t) => r.tags.includes(t))
      );
    }
    if (this.state.maxResults) {
      results = results.slice(0, this.state.maxResults);
    }
    return results;
  }
  async _resolveObjects() {
    const filter5 = this.state.tagFilters.length > 0 ? { tags: this.state.tagFilters } : this.state.jsonPath ? { jsonPath: this.state.jsonPath, jsonValue: this.state.jsonValue } : void 0;
    let results = await this.run(
      this.store.query(this.state.ns, filter5)
    );
    if (this.state.maxResults) {
      results = results.slice(0, this.state.maxResults);
    }
    return results;
  }
};
var PutBuilder = class {
  store;
  run;
  state;
  constructor(store, run2, ns) {
    this.store = store;
    this.run = run2;
    this.state = { ns, timestamped: false, tags: [] };
  }
  /** Set the key */
  key(k) {
    this.state.key = k;
    return this;
  }
  /** Auto-append --YYYYMMDDTHHMMSS to key */
  timestamped() {
    this.state.timestamped = true;
    return this;
  }
  /** Set the data payload */
  data(d) {
    this.state.data = d;
    return this;
  }
  /** Set _meta fields */
  meta(m) {
    this.state.meta = m;
    return this;
  }
  /** Add tags */
  tags(...t) {
    this.state.tags.push(...t);
    return this;
  }
  // ── Terminal ─────────────────────────────────────────────────
  /** Validate and store. Returns { ns, key }. */
  async put() {
    const { ns, key, timestamped, data, meta, tags } = this.state;
    if (!key) throw new Error("PutBuilder: key is required. Call .key('name') first.");
    if (!data) throw new Error("PutBuilder: data is required. Call .data({...}) first.");
    const finalKey = timestamped ? key + temporalSuffix() : key;
    validateNamespace(ns);
    validateKey(finalKey);
    if (meta) validateMeta(meta);
    const envelope = { ...data };
    if (meta) {
      envelope._meta = meta;
    }
    return this.run(
      this.store.put(ns, finalKey, envelope, tags.length > 0 ? { tags } : void 0)
    ).then(() => ({ ns, key: finalKey }));
  }
};

// src/store/api.ts
function createStoreApi(sqlLayer, fsLayer) {
  const ServiceLayers = fsLayer ? mergeAll2(RlmStoreLive, SearchIndexLive, DomainRegistryLive, ExportServiceLive) : mergeAll2(RlmStoreLive, SearchIndexLive, DomainRegistryLive);
  const baseLayers = ServiceLayers.pipe(
    provide2(RlmStoreLive),
    provide2(MigrationLayer),
    provide2(sqlLayer)
  );
  const AppLayer = fsLayer ? baseLayers.pipe(provide2(fsLayer)) : baseLayers;
  const runtime = make16(AppLayer);
  const run2 = (effect2) => runtime.runPromise(effect2);
  const withStore = (f) => run2(gen2(function* () {
    const store = yield* RlmStore;
    return yield* f(store);
  }));
  const withSearch = (f) => run2(gen2(function* () {
    const search = yield* SearchIndex;
    return yield* f(search);
  }));
  const withDomains = (f) => run2(gen2(function* () {
    const domains = yield* DomainRegistry;
    return yield* f(domains);
  }));
  let _storeRef = null;
  let _searchRef = null;
  const getServiceRefs = async () => {
    if (!_storeRef) {
      const refs = await run2(gen2(function* () {
        const store = yield* RlmStore;
        const search = yield* SearchIndex;
        return { store, search };
      }));
      _storeRef = refs.store;
      _searchRef = refs.search;
    }
    return { store: _storeRef, search: _searchRef };
  };
  const builderRun = (effect2) => runtime.runPromise(effect2);
  return {
    // ── Core CRUD ──────────────────────────────────────────────
    put: (collection, key, data, tags) => run2(gen2(function* () {
      const store = yield* RlmStore;
      const search = yield* SearchIndex;
      yield* store.put(collection, key, data, tags ? { tags } : void 0);
      yield* search.notify(collection, key, data, tags ?? []);
    })).then(() => void 0),
    putNow: (collection, prefix, data, tags) => run2(gen2(function* () {
      const store = yield* RlmStore;
      const search = yield* SearchIndex;
      const result2 = yield* store.putNow(collection, prefix, data, tags ? { tags } : void 0);
      yield* search.notify(collection, result2.key, data, tags ?? []);
      return result2;
    })),
    get: (collection, key) => withStore((s) => s.get(collection, key)),
    getRaw: (collection, key) => withStore((s) => s.getRaw(collection, key)),
    describe: (collection, key) => withStore((s) => s.describe(collection, key)),
    delete: (collection, key) => run2(gen2(function* () {
      const store = yield* RlmStore;
      const search = yield* SearchIndex;
      const result2 = yield* store.del(collection, key);
      yield* search.notifyRemove(collection, key);
      return result2;
    })),
    // ── Query ──────────────────────────────────────────────────
    query: (collection, filter5) => withStore((s) => s.query(collection, filter5)),
    keys: (collection) => withStore((s) => s.keys(collection)),
    catalog: (nsGlob) => withStore((s) => s.catalog(nsGlob)),
    vars: () => withStore((s) => s.vars()),
    search: (text, nsGlob) => withSearch((s) => s.search(text, nsGlob)),
    // ── Collections ────────────────────────────────────────────
    collections: (glob) => withStore((s) => s.collections(glob)),
    clear: (collection) => withStore((s) => s.clear(collection)),
    // ── Domains ────────────────────────────────────────────────
    domain: (name, config) => {
      const validated = validateDomainConfig(config);
      return withDomains((d) => d.register(name, validated)).then(() => void 0);
    },
    domains: () => withDomains((d) => d.list()),
    // ── Fluent Query ───────────────────────────────────────────
    from: (ns) => {
      const state = {
        tagFilters: [],
        searchText: void 0,
        maxResults: void 0,
        jsonPath: void 0,
        jsonValue: void 0
      };
      const chain = {
        tagged(...tags) {
          state.tagFilters.push(...tags);
          return chain;
        },
        search(text) {
          state.searchText = text;
          return chain;
        },
        limit(n) {
          state.maxResults = n;
          return chain;
        },
        where(path, value) {
          state.jsonPath = path;
          state.jsonValue = value;
          return chain;
        },
        async keys() {
          const { store, search } = await getServiceRefs();
          const b = new QueryBuilder(store, search, builderRun, ns);
          state.tagFilters.forEach((t) => b.tagged(t));
          if (state.searchText) b.search(state.searchText);
          if (state.maxResults) b.limit(state.maxResults);
          if (state.jsonPath) b.where(state.jsonPath, state.jsonValue);
          return b.keys();
        },
        async entries() {
          const { store, search } = await getServiceRefs();
          const b = new QueryBuilder(store, search, builderRun, ns);
          state.tagFilters.forEach((t) => b.tagged(t));
          if (state.searchText) b.search(state.searchText);
          if (state.maxResults) b.limit(state.maxResults);
          if (state.jsonPath) b.where(state.jsonPath, state.jsonValue);
          return b.entries();
        },
        async summaries() {
          const { store, search } = await getServiceRefs();
          const b = new QueryBuilder(store, search, builderRun, ns);
          state.tagFilters.forEach((t) => b.tagged(t));
          if (state.searchText) b.search(state.searchText);
          if (state.maxResults) b.limit(state.maxResults);
          if (state.jsonPath) b.where(state.jsonPath, state.jsonValue);
          return b.summaries();
        },
        async count() {
          const { store, search } = await getServiceRefs();
          const b = new QueryBuilder(store, search, builderRun, ns);
          state.tagFilters.forEach((t) => b.tagged(t));
          if (state.searchText) b.search(state.searchText);
          if (state.maxResults) b.limit(state.maxResults);
          if (state.jsonPath) b.where(state.jsonPath, state.jsonValue);
          return b.count();
        }
      };
      return chain;
    },
    // ── Fluent Put ─────────────────────────────────────────────
    into: (ns) => {
      const state = {
        key: void 0,
        timestamped: false,
        data: void 0,
        meta: void 0,
        tags: []
      };
      const chain = {
        key(k) {
          state.key = k;
          return chain;
        },
        timestamped() {
          state.timestamped = true;
          return chain;
        },
        data(d) {
          state.data = d;
          return chain;
        },
        meta(m) {
          state.meta = m;
          return chain;
        },
        tags(...t) {
          state.tags.push(...t);
          return chain;
        },
        async put() {
          const { store } = await getServiceRefs();
          const b = new PutBuilder(store, builderRun, ns);
          if (state.key) b.key(state.key);
          if (state.timestamped) b.timestamped();
          if (state.data) b.data(state.data);
          if (state.meta) b.meta(state.meta);
          if (state.tags.length > 0) b.tags(...state.tags);
          return b.put();
        }
      };
      return chain;
    },
    // ── Backward Compat ────────────────────────────────────────
    store: (collection, key, data, tags) => withStore((s) => s.put(collection, key, data, tags ? { tags } : void 0)).then(() => void 0),
    // ── Export / Import ────────────────────────────────────────
    exportStore: (opts) => {
      if (!fsLayer) return Promise.reject(new Error("Export requires a FileSystem layer. Pass fsLayer to createStoreApi()."));
      return run2(gen2(function* () {
        const exp = yield* ExportService;
        return yield* exp.exportStore(opts);
      }));
    },
    importStore: (opts) => {
      if (!fsLayer) return Promise.reject(new Error("Import requires a FileSystem layer. Pass fsLayer to createStoreApi()."));
      return run2(gen2(function* () {
        const exp = yield* ExportService;
        return yield* exp.importStore(opts);
      }));
    },
    profiles: () => {
      if (!fsLayer) return Promise.reject(new Error("Profiles require a FileSystem layer. Pass fsLayer to createStoreApi()."));
      return run2(gen2(function* () {
        const exp = yield* ExportService;
        return yield* exp.listProfiles();
      }));
    },
    removeProfile: (name) => {
      if (!fsLayer) return Promise.reject(new Error("removeProfile requires a FileSystem layer. Pass fsLayer to createStoreApi()."));
      return run2(gen2(function* () {
        const exp = yield* ExportService;
        return yield* exp.removeProfile(name);
      }));
    },
    // ── Lifecycle ──────────────────────────────────────────────
    dispose: () => runtime.dispose()
  };
}

// src/store/procedures.ts
var COLLECTION = "_system.procedures";
function toStorageKey(name) {
  return name.replace(/([a-z0-9])([A-Z])/g, "$1-$2").replace(/([A-Z]+)([A-Z][a-z])/g, "$1-$2").replace(/_/g, "-").toLowerCase();
}
function createProcedureApi(storeGet, storePut, storeDelete, storeQuery, storeKeys, getMsObject) {
  async function define(name, fn2, opts) {
    const code = fn2.toString();
    return defineCode(name, code, opts);
  }
  async function defineCode(name, code, opts) {
    if (!name || typeof name !== "string") throw new Error("Procedure name must be a non-empty string");
    if (!code || typeof code !== "string") throw new Error("Procedure code must be a non-empty string");
    const key = toStorageKey(name);
    const existing = await storeGet(COLLECTION, key);
    const version2 = existing ? (existing.version ?? 0) + 1 : 1;
    const now = (/* @__PURE__ */ new Date()).toISOString();
    const manifest = opts?.manifest || `ms.fn.${name}(args?) \u2192 (see ms.describeProcedure("${name}"))`;
    const record2 = {
      name,
      description: opts?.description ?? "",
      manifest,
      code,
      tags: opts?.tags ?? [],
      version: version2,
      author: opts?.author ?? "agent",
      created: existing?.created ?? now,
      updated: now,
      dependencies: opts?.dependencies ?? [],
      ...opts?.inputSchema ? { inputSchema: opts.inputSchema } : {},
      ...opts?.outputSchema ? { outputSchema: opts.outputSchema } : {}
    };
    const summary = record2.description ? `[proc v${version2}] ${record2.description}` : `[proc v${version2}] ${name}`;
    const storePayload = {
      ...record2,
      _meta: { summary, source: "dpa", type: "procedure" }
    };
    await storePut(COLLECTION, key, storePayload, [
      "procedure",
      ...record2.tags
    ]);
    return record2;
  }
  async function call(name, args2) {
    const key = toStorageKey(name);
    const record2 = toProcedureRecord(await storeGet(COLLECTION, key));
    if (!record2) throw new Error(`Procedure '${name}' not found. Use ms.procedures() to list available.`);
    const code = record2.code;
    const ms = getMsObject();
    const start = Date.now();
    try {
      const fn2 = reconstructFunction(code);
      const result2 = await fn2(ms, args2);
      return result2;
    } catch (err) {
      throw new Error(`Procedure '${name}' failed: ${err.message}`);
    }
  }
  async function procedures() {
    const keys = await storeKeys(COLLECTION);
    const summaries = [];
    for (const key of keys) {
      const record2 = toProcedureRecord(await storeGet(COLLECTION, key));
      if (record2) {
        summaries.push({
          name: record2.name,
          description: record2.description,
          manifest: record2.manifest ?? `ms.fn.${record2.name}(args?)`,
          version: record2.version,
          tags: record2.tags,
          dependencies: record2.dependencies,
          hasInputSchema: !!record2.inputSchema,
          hasOutputSchema: !!record2.outputSchema,
          updated: record2.updated
        });
      }
    }
    return summaries.sort((a, b) => a.name.localeCompare(b.name));
  }
  function toProcedureRecord(raw) {
    if (!raw) return null;
    const { _meta, ...record2 } = raw;
    return record2;
  }
  async function describeProcedure(name) {
    const key = toStorageKey(name);
    const raw = await storeGet(COLLECTION, key);
    return toProcedureRecord(raw);
  }
  async function remove(name) {
    const key = toStorageKey(name);
    return storeDelete(COLLECTION, key);
  }
  async function source(name) {
    const key = toStorageKey(name);
    const record2 = toProcedureRecord(await storeGet(COLLECTION, key));
    return record2?.code ?? null;
  }
  const fnProxy = new Proxy({}, {
    get(_target, prop) {
      if (typeof prop !== "string") return void 0;
      return (args2) => call(prop, args2);
    },
    has(_target, prop) {
      return typeof prop === "string";
    },
    ownKeys() {
      return [];
    }
  });
  return {
    define,
    defineCode,
    call,
    procedures,
    describe: describeProcedure,
    remove,
    source,
    fn: fnProxy
  };
}
function reconstructFunction(code) {
  const trimmed = code.trim();
  if (trimmed.startsWith("(") || trimmed.startsWith("function") || trimmed.startsWith("async (") || trimmed.startsWith("async function")) {
    try {
      const fn2 = new Function(`"use strict"; return (${trimmed})`)();
      if (typeof fn2 === "function") return fn2;
    } catch {
    }
  }
  if (/^(?:async\s+)?[a-zA-Z_$]\w*\s*=>/.test(trimmed)) {
    try {
      const fn2 = new Function(`"use strict"; return (${trimmed})`)();
      if (typeof fn2 === "function") return fn2;
    } catch {
    }
  }
  return new Function("cm", "args", `"use strict"; const ms = cm; ${trimmed}`);
}

// src/primitives/io.ts
function createIoApi(cwd, fsLayer, runtimeLayer = makeNodeRuntimeLayer(cwd)) {
  const appLayer = runtimeLayer.pipe(provide2(fsLayer));
  const runtime = make16(appLayer);
  const run2 = (effect2) => runtime.runPromise(effect2);
  const read = (path) => run2(MetatoolRuntime.pipe(
    flatMap3((rt) => rt.read(path))
  ));
  const write = (path, content) => run2(MetatoolRuntime.pipe(
    flatMap3((rt) => rt.write(path, content))
  ));
  const sh = (cmd) => run2(MetatoolRuntime.pipe(
    flatMap3((rt) => rt.exec({ shell: cmd })),
    map5((result2) => `${result2.stdout ?? ""}${result2.stderr ?? ""}`.trim()),
    catch_2((e) => succeed5(e.message))
  ));
  const execRuntime = (command, options) => run2(MetatoolRuntime.pipe(
    flatMap3((rt) => rt.exec(command, options))
  ));
  const dispose = () => runtime.dispose();
  return { read, write, sh, exec: execRuntime, dispose };
}

// src/overlay.ts
var OverlayManager = class {
  stack = [];
  _compiled = emptyCompiled();
  core = null;
  onRecompile;
  constructor(options) {
    this.onRecompile = options?.onRecompile;
  }
  /** Bind the core reference — called once during metatool init */
  setCore(core) {
    this.core = core;
  }
  /** Load an overlay onto the top of the stack */
  async load(overlay) {
    if (this.stack.some((o) => o.id === overlay.id)) {
      throw new Error(`Overlay "${overlay.id}" is already loaded`);
    }
    this.stack.push(overlay);
    if (overlay.lifecycle?.onLoad && this.core) {
      await overlay.lifecycle.onLoad(this.core);
    }
    await this.seedProcedures(overlay);
    await this.recompile();
  }
  /** Unload a specific overlay by id */
  async unload(id) {
    const idx = this.stack.findIndex((o) => o.id === id);
    if (idx === -1) {
      throw new Error(`Overlay "${id}" is not loaded`);
    }
    const overlay = this.stack[idx];
    if (overlay.lifecycle?.onUnload) {
      await overlay.lifecycle.onUnload();
    }
    if (overlay.dispose) {
      await overlay.dispose();
    }
    this.stack.splice(idx, 1);
    await this.recompile();
  }
  /** Load multiple overlays in stack order */
  async loadBatch(overlays) {
    for (const overlay of overlays) {
      if (this.stack.some((o) => o.id === overlay.id)) {
        throw new Error(`Overlay "${overlay.id}" is already loaded`);
      }
      this.stack.push(overlay);
      if (overlay.lifecycle?.onLoad && this.core) {
        await overlay.lifecycle.onLoad(this.core);
      }
      await this.seedProcedures(overlay);
    }
    await this.recompile();
  }
  /** Unload all overlays (top-down order for lifecycle) */
  async clear() {
    for (let i = this.stack.length - 1; i >= 0; i--) {
      const overlay = this.stack[i];
      if (overlay.lifecycle?.onUnload) {
        await overlay.lifecycle.onUnload();
      }
      if (overlay.dispose) {
        await overlay.dispose();
      }
    }
    this.stack = [];
    await this.recompile();
  }
  /** Switch to a single overlay (clear + load) */
  async switchTo(overlay) {
    await this.clear();
    await this.load(overlay);
  }
  /** List active overlays in stack order */
  active() {
    return this.stack.map((o) => ({ id: o.id, name: o.name, version: o.version }));
  }
  /** Check if an overlay is loaded */
  has(id) {
    return this.stack.some((o) => o.id === id);
  }
  /** Get the compiled state snapshot */
  compiled() {
    return this._compiled;
  }
  /** Get an overlay by id (or undefined) */
  get(id) {
    return this.stack.find((o) => o.id === id);
  }
  /** Number of loaded overlays */
  get size() {
    return this.stack.length;
  }
  // ── Procedure seeding ───────────────────────────────────
  async seedProcedures(overlay) {
    if (!this.core || !overlay.procedures) return;
    for (const procedure of overlay.procedures) {
      const code = procedure.fn.toString();
      const tags = [`overlay:${overlay.id}`, ...procedure.tags ?? []];
      const existing = await this.core.procedures.describe(procedure.name);
      const unchanged = existing !== null && existing.code === code && existing.manifest === procedure.manifest && existing.tags.length === tags.length && existing.tags.every((tag2, index) => tag2 === tags[index]);
      if (unchanged) continue;
      await this.core.procedures.define(procedure.name, procedure.fn, {
        manifest: procedure.manifest,
        tags,
        author: `overlay:${overlay.id}`
      });
    }
  }
  // ── Recompile ────────────────────────────────────────────
  async recompile() {
    this._compiled = compile(this.stack);
    if (this.onRecompile) {
      await this.onRecompile(this._compiled);
    }
  }
};
function compile(stack) {
  const methods = {};
  const guideSections = [];
  const guidePriorities = {};
  const steerFragments = [];
  const steerSuppress = [];
  const profiles = [];
  const profileBundles = {};
  const procedures = [];
  const contextFields = {};
  let contextReplace = false;
  const renderers = {};
  const layout = {};
  const grid = {};
  const errorFormatters = {};
  const stackIds = [];
  for (const overlay of stack) {
    stackIds.push(overlay.id);
    Object.assign(methods, overlay.methods);
    if (overlay.guide) {
      guideSections.push(...overlay.guide.sections);
      if (overlay.guide.priorities) {
        Object.assign(guidePriorities, overlay.guide.priorities);
      }
    }
    if (overlay.steer) {
      for (const frag of overlay.steer.fragments) {
        steerFragments.push({ ...frag, overlayId: overlay.id });
      }
      if (overlay.steer.suppress) {
        steerSuppress.push({ ...overlay.steer.suppress, overlayId: overlay.id });
      }
    }
    if (overlay.profiles) {
      for (const p of overlay.profiles.autoLoad) {
        if (!profiles.includes(p)) profiles.push(p);
      }
      if (overlay.profiles.bundles) {
        Object.assign(profileBundles, overlay.profiles.bundles);
      }
    }
    if (overlay.procedures) {
      for (const proc of overlay.procedures) {
        procedures.push({ ...proc, overlayId: overlay.id });
      }
    }
    if (overlay.context) {
      Object.assign(contextFields, overlay.context.fields);
      if (overlay.context.replace) contextReplace = true;
    }
    if (overlay.rendering) {
      if (overlay.rendering.renderers) Object.assign(renderers, overlay.rendering.renderers);
      if (overlay.rendering.layout) Object.assign(layout, overlay.rendering.layout);
      if (overlay.rendering.grid) Object.assign(grid, overlay.rendering.grid);
    }
    if (overlay.errors) {
      Object.assign(errorFormatters, overlay.errors.formatters);
    }
  }
  steerFragments.sort((a, b) => (a.priority ?? 50) - (b.priority ?? 50));
  return {
    methods,
    guideSections,
    guidePriorities,
    steerFragments,
    steerSuppress,
    profiles,
    profileBundles,
    procedures,
    contextFields,
    contextReplace,
    renderers,
    layout,
    grid,
    errorFormatters,
    stack: stackIds
  };
}
function emptyCompiled() {
  return {
    methods: {},
    guideSections: [],
    guidePriorities: {},
    steerFragments: [],
    steerSuppress: [],
    profiles: [],
    profileBundles: {},
    procedures: [],
    contextFields: {},
    contextReplace: false,
    renderers: {},
    layout: {},
    grid: {},
    errorFormatters: {},
    stack: []
  };
}
function pluginToOverlay(plugin) {
  return {
    id: plugin.id,
    name: plugin.name,
    methods: plugin.methods,
    guide: plugin.manifest ? { sections: [plugin.manifest] } : void 0,
    lifecycle: plugin.setup ? { onLoad: plugin.setup } : void 0,
    dispose: plugin.dispose
  };
}

// src/index.ts
async function createMetatool(options) {
  const { sqlLayer, fsLayer, runtimeLayer, overlays: overlayInputs = [], plugins = [], cwd } = options;
  const store = createStoreApi(sqlLayer, fsLayer);
  const io = createIoApi(cwd, fsLayer, runtimeLayer);
  let _api = {};
  const procApi = createProcedureApi(
    store.get.bind(store),
    store.put.bind(store),
    store.delete.bind(store),
    store.query.bind(store),
    store.keys.bind(store),
    () => _api
  );
  const core = {
    store,
    procedures: procApi,
    cwd,
    read: io.read,
    write: io.write,
    sh: io.sh,
    exec: io.exec,
    runtime: MetatoolRuntime
  };
  const coreMethods = {
    // Store (14)
    store: store.store.bind(store),
    put: store.put.bind(store),
    putNow: store.putNow.bind(store),
    get: store.get.bind(store),
    getRaw: store.getRaw.bind(store),
    describe: store.describe.bind(store),
    query: store.query.bind(store),
    keys: store.keys.bind(store),
    delete: store.delete.bind(store),
    collections: store.collections.bind(store),
    clear: store.clear.bind(store),
    vars: store.vars.bind(store),
    catalog: store.catalog.bind(store),
    search: store.search.bind(store),
    // Builders (2)
    from: store.from.bind(store),
    into: store.into.bind(store),
    // Domains (2)
    domain: store.domain.bind(store),
    domains: store.domains.bind(store),
    // Portability (4)
    exportStore: store.exportStore.bind(store),
    importStore: store.importStore.bind(store),
    profiles: store.profiles.bind(store),
    removeProfile: store.removeProfile.bind(store),
    // Procedures / DPA (8)
    define: procApi.define,
    defineCode: procApi.defineCode,
    call: procApi.call,
    procedures: procApi.procedures,
    describeProcedure: procApi.describe,
    removeProcedure: procApi.remove,
    source: procApi.source,
    fn: procApi.fn,
    // Primitives (3) — async, backed by Effect FileSystem
    read: io.read,
    write: io.write,
    sh: io.sh,
    exec: io.exec
  };
  function rebuildApi(compiled) {
    for (const k of Object.keys(_api)) delete _api[k];
    Object.assign(_api, coreMethods, compiled.methods, overlayOps);
  }
  const overlayManager = new OverlayManager({
    onRecompile: rebuildApi
  });
  overlayManager.setCore(core);
  const overlayOps = {
    loadOverlay: async (overlay) => {
      await overlayManager.load(overlay);
    },
    unloadOverlay: async (id) => {
      await overlayManager.unload(id);
    },
    switchOverlay: async (overlay) => {
      await overlayManager.switchTo(overlay);
    },
    overlays: () => overlayManager.active(),
    hasOverlay: (id) => overlayManager.has(id)
  };
  const allOverlays = [
    ...overlayInputs,
    ...plugins.map(pluginToOverlay)
  ];
  if (allOverlays.length > 0) {
    await overlayManager.loadBatch(allOverlays);
  }
  Object.assign(_api, coreMethods, overlayManager.compiled().methods, overlayOps);
  async function evalCode(code) {
    let transformed = code;
    for (const overlay of allOverlays) {
      if (overlay.lifecycle?.onEval) {
        transformed = overlay.lifecycle.onEval(transformed);
      }
    }
    const fn2 = new Function("cm", `"use strict"; const ms = cm; return (async () => { ${transformed} })()`);
    let result2 = await fn2(_api);
    for (const overlay of allOverlays) {
      if (overlay.lifecycle?.onResult) {
        result2 = overlay.lifecycle.onResult(result2);
      }
    }
    return result2;
  }
  async function dispose() {
    await overlayManager.clear();
    await io.dispose();
    await store.dispose();
  }
  return {
    api: _api,
    /** Always-fresh API snapshot merging core + overlay methods + overlay ops */
    getApi() {
      return { ...coreMethods, ...overlayManager.compiled().methods, ...overlayOps };
    },
    eval: evalCode,
    core,
    plugins: overlayManager.compiled().stack,
    overlays: overlayManager,
    dispose
  };
}

// node_modules/effect/dist/unstable/sql/SqlError.js
var TypeId23 = "~effect/sql/SqlError";
var ReasonTypeId = "~effect/sql/SqlError/Reason";
var ReasonFields = {
  cause: /* @__PURE__ */ Defect(),
  message: /* @__PURE__ */ optional(String4),
  operation: /* @__PURE__ */ optional(String4)
};
var ConnectionError = class extends (/* @__PURE__ */ TaggedErrorClass("effect/sql/SqlError/ConnectionError")("ConnectionError", ReasonFields)) {
  /**
   * Marks this value as a structured SQL error reason for runtime guards.
   *
   * @since 4.0.0
   */
  [ReasonTypeId] = ReasonTypeId;
  /**
   * Indicates whether retrying the failed SQL operation may succeed.
   *
   * @since 4.0.0
   */
  get isRetryable() {
    return true;
  }
};
var AuthenticationError = class extends (/* @__PURE__ */ TaggedErrorClass("effect/sql/SqlError/AuthenticationError")("AuthenticationError", ReasonFields)) {
  /**
   * Marks this value as a structured SQL error reason for runtime guards.
   *
   * @since 4.0.0
   */
  [ReasonTypeId] = ReasonTypeId;
  /**
   * Indicates whether retrying the failed SQL operation may succeed.
   *
   * @since 4.0.0
   */
  get isRetryable() {
    return false;
  }
};
var AuthorizationError = class extends (/* @__PURE__ */ TaggedErrorClass("effect/sql/SqlError/AuthorizationError")("AuthorizationError", ReasonFields)) {
  /**
   * Marks this value as a structured SQL error reason for runtime guards.
   *
   * @since 4.0.0
   */
  [ReasonTypeId] = ReasonTypeId;
  /**
   * Indicates whether retrying the failed SQL operation may succeed.
   *
   * @since 4.0.0
   */
  get isRetryable() {
    return false;
  }
};
var SqlSyntaxError = class extends (/* @__PURE__ */ TaggedErrorClass("effect/sql/SqlError/SqlSyntaxError")("SqlSyntaxError", ReasonFields)) {
  /**
   * Marks this value as a structured SQL error reason for runtime guards.
   *
   * @since 4.0.0
   */
  [ReasonTypeId] = ReasonTypeId;
  /**
   * Indicates whether retrying the failed SQL operation may succeed.
   *
   * @since 4.0.0
   */
  get isRetryable() {
    return false;
  }
};
var UniqueViolationFields = {
  ...ReasonFields,
  constraint: String4
};
var UniqueViolation = class extends (/* @__PURE__ */ TaggedErrorClass("effect/sql/SqlError/UniqueViolation")("UniqueViolation", UniqueViolationFields)) {
  /**
   * Marks this value as a structured SQL error reason for runtime guards.
   *
   * @since 4.0.0
   */
  [ReasonTypeId] = ReasonTypeId;
  /**
   * Indicates whether retrying the failed SQL operation may succeed.
   *
   * @since 4.0.0
   */
  get isRetryable() {
    return false;
  }
};
var ConstraintError = class extends (/* @__PURE__ */ TaggedErrorClass("effect/sql/SqlError/ConstraintError")("ConstraintError", ReasonFields)) {
  /**
   * Marks this value as a structured SQL error reason for runtime guards.
   *
   * @since 4.0.0
   */
  [ReasonTypeId] = ReasonTypeId;
  /**
   * Indicates whether retrying the failed SQL operation may succeed.
   *
   * @since 4.0.0
   */
  get isRetryable() {
    return false;
  }
};
var DeadlockError = class extends (/* @__PURE__ */ TaggedErrorClass("effect/sql/SqlError/DeadlockError")("DeadlockError", ReasonFields)) {
  /**
   * Marks this value as a structured SQL error reason for runtime guards.
   *
   * @since 4.0.0
   */
  [ReasonTypeId] = ReasonTypeId;
  /**
   * Indicates whether retrying the failed SQL operation may succeed.
   *
   * @since 4.0.0
   */
  get isRetryable() {
    return true;
  }
};
var SerializationError = class extends (/* @__PURE__ */ TaggedErrorClass("effect/sql/SqlError/SerializationError")("SerializationError", ReasonFields)) {
  /**
   * Marks this value as a structured SQL error reason for runtime guards.
   *
   * @since 4.0.0
   */
  [ReasonTypeId] = ReasonTypeId;
  /**
   * Indicates whether retrying the failed SQL operation may succeed.
   *
   * @since 4.0.0
   */
  get isRetryable() {
    return true;
  }
};
var LockTimeoutError = class extends (/* @__PURE__ */ TaggedErrorClass("effect/sql/SqlError/LockTimeoutError")("LockTimeoutError", ReasonFields)) {
  /**
   * Marks this value as a structured SQL error reason for runtime guards.
   *
   * @since 4.0.0
   */
  [ReasonTypeId] = ReasonTypeId;
  /**
   * Indicates whether retrying the failed SQL operation may succeed.
   *
   * @since 4.0.0
   */
  get isRetryable() {
    return true;
  }
};
var StatementTimeoutError = class extends (/* @__PURE__ */ TaggedErrorClass("effect/sql/SqlError/StatementTimeoutError")("StatementTimeoutError", ReasonFields)) {
  /**
   * Marks this value as a structured SQL error reason for runtime guards.
   *
   * @since 4.0.0
   */
  [ReasonTypeId] = ReasonTypeId;
  /**
   * Indicates whether retrying the failed SQL operation may succeed.
   *
   * @since 4.0.0
   */
  get isRetryable() {
    return true;
  }
};
var UnknownError2 = class extends (/* @__PURE__ */ TaggedErrorClass("effect/sql/SqlError/UnknownError")("UnknownError", ReasonFields)) {
  /**
   * Marks this value as a structured SQL error reason for runtime guards.
   *
   * @since 4.0.0
   */
  [ReasonTypeId] = ReasonTypeId;
  /**
   * Indicates whether retrying the failed SQL operation may succeed.
   *
   * @since 4.0.0
   */
  get isRetryable() {
    return false;
  }
};
var SqlErrorReason = /* @__PURE__ */ Union2([ConnectionError, AuthenticationError, AuthorizationError, SqlSyntaxError, UniqueViolation, ConstraintError, DeadlockError, SerializationError, LockTimeoutError, StatementTimeoutError, UnknownError2]);
var SqlError = class extends (/* @__PURE__ */ TaggedErrorClass("effect/sql/SqlError")("SqlError", {
  reason: SqlErrorReason
})) {
  /**
   * Marks this value as the top-level SQL error wrapper for runtime guards.
   *
   * @since 4.0.0
   */
  [TypeId23] = TypeId23;
  /**
   * Exposes the structured SQL reason as the JavaScript error cause.
   *
   * @since 4.0.0
   */
  cause = this.reason;
  /**
   * Uses the reason message when present, otherwise falls back to the reason tag.
   *
   * @since 4.0.0
   */
  get message() {
    return this.reason.message || this.reason._tag;
  }
  /**
   * Delegates retryability to the underlying SQL error reason.
   *
   * @since 4.0.0
   */
  get isRetryable() {
    return this.reason.isRetryable;
  }
};
var sqliteCodeFromCause = (cause) => {
  if (!hasProperty(cause, "code")) {
    return void 0;
  }
  const code = cause.code;
  return typeof code === "string" || typeof code === "number" ? code : void 0;
};
var sqliteNumericCodeFromCause = (cause) => {
  const code = sqliteCodeFromCause(cause);
  if (typeof code === "number") {
    return code;
  }
  if (!hasProperty(cause, "errno")) {
    return void 0;
  }
  const errno = cause.errno;
  return typeof errno === "number" ? errno : void 0;
};
var matchesSqliteNumericCode = (cause, expected) => {
  const code = sqliteCodeFromCause(cause);
  if (code === expected) {
    return true;
  }
  if (!hasProperty(cause, "errno")) {
    return false;
  }
  return cause.errno === expected;
};
var matchesSqliteCode = (code, expected) => code === expected || code.startsWith(expected + "_");
var UNKNOWN_CONSTRAINT = "unknown";
var SQLITE_CONSTRAINT_UNIQUE = "SQLITE_CONSTRAINT_UNIQUE";
var SQLITE_CONSTRAINT_UNIQUE_CODE = 2067;
var normalizeConstraintIdentifier = (identifier3) => {
  if (typeof identifier3 !== "string") {
    return UNKNOWN_CONSTRAINT;
  }
  const trimmed = identifier3.trim();
  return trimmed.length === 0 ? UNKNOWN_CONSTRAINT : trimmed;
};
var sqliteUniqueConstraintFromCause = (cause) => {
  if (hasProperty(cause, "constraint")) {
    return normalizeConstraintIdentifier(cause.constraint);
  }
  if (!hasProperty(cause, "message")) {
    return UNKNOWN_CONSTRAINT;
  }
  const message = cause.message;
  if (typeof message !== "string") {
    return UNKNOWN_CONSTRAINT;
  }
  const prefix = "UNIQUE constraint failed:";
  const index = message.indexOf(prefix);
  return index === -1 ? UNKNOWN_CONSTRAINT : normalizeConstraintIdentifier(message.slice(index + prefix.length));
};
var classifySqliteError = (cause, {
  message,
  operation
} = {}) => {
  const props = {
    cause,
    message,
    operation
  };
  const code = sqliteCodeFromCause(cause);
  const numericCode = sqliteNumericCodeFromCause(cause);
  if (code === SQLITE_CONSTRAINT_UNIQUE || matchesSqliteNumericCode(cause, SQLITE_CONSTRAINT_UNIQUE_CODE)) {
    return new UniqueViolation({
      ...props,
      constraint: sqliteUniqueConstraintFromCause(cause)
    });
  }
  if (typeof code === "string") {
    if (matchesSqliteCode(code, "SQLITE_AUTH")) {
      return new AuthenticationError(props);
    }
    if (matchesSqliteCode(code, "SQLITE_PERM")) {
      return new AuthorizationError(props);
    }
    if (matchesSqliteCode(code, "SQLITE_CONSTRAINT")) {
      return new ConstraintError(props);
    }
    if (matchesSqliteCode(code, "SQLITE_BUSY") || matchesSqliteCode(code, "SQLITE_LOCKED")) {
      return new LockTimeoutError(props);
    }
    if (matchesSqliteCode(code, "SQLITE_CANTOPEN")) {
      return new ConnectionError(props);
    }
  }
  if (typeof numericCode === "number") {
    const code2 = numericCode & 255;
    switch (code2) {
      case 23:
        return new AuthenticationError(props);
      case 3:
        return new AuthorizationError(props);
      case 19:
        return new ConstraintError(props);
      case 5:
      case 6:
        return new LockTimeoutError(props);
      case 14:
        return new ConnectionError(props);
      default:
        return new UnknownError2(props);
    }
  }
  return new UnknownError2(props);
};

// src/adapters/sqlite-node.ts
import { DatabaseSync } from "node:sqlite";
var ReactivityNoop = effect(Reactivity)(
  sync2(
    () => Reactivity.of({
      invalidateUnsafe: () => {
      },
      registerUnsafe: () => () => {
      },
      invalidate: () => void_3,
      mutation: (_keys, effect2) => effect2,
      query: () => die3("Reactivity not available in RLM store"),
      stream: () => {
        throw new Error("Reactivity not available in RLM store");
      },
      withBatch: (effect2) => effect2
    })
  )
);
var layer = (config) => {
  const makeClient = gen2(function* () {
    const compiler = makeCompilerSqlite(config.transformQueryNames);
    const transformRows = config.transformResultNames ? defaultTransforms(config.transformResultNames).array : void 0;
    const db = new DatabaseSync(config.filename, {
      readOnly: config.readonly ?? false
    });
    yield* addFinalizer3(() => sync2(() => db.close()));
    if (config.disableWAL !== true) {
      db.exec("PRAGMA journal_mode = WAL");
    }
    const run2 = (sql, params = []) => try_2({
      try: () => {
        const stmt = db.prepare(sql);
        try {
          const sqlParams = params.map((param) => typeof param === "boolean" ? Number(param) : param);
          return stmt.all(...sqlParams);
        } catch {
          const sqlParams = params.map((param) => typeof param === "boolean" ? Number(param) : param);
          stmt.run(...sqlParams);
          return [];
        }
      },
      catch: (cause) => new SqlError({ reason: classifySqliteError(cause, { message: "Failed to execute statement", operation: sql }) })
    });
    const connection = {
      execute(sql, params, transformRows2) {
        return transformRows2 ? map5(run2(sql, params), transformRows2) : run2(sql, params);
      },
      executeRaw(sql, params) {
        return run2(sql, params);
      },
      executeValues(sql, params) {
        return map5(
          run2(sql, params),
          (rows) => rows.map((r) => Object.values(r))
        );
      },
      executeValuesUnprepared(sql, params) {
        return this.executeValues(sql, params ?? []);
      },
      executeUnprepared(sql, params, transformRows2) {
        return this.execute(sql, params ?? [], transformRows2);
      },
      executeStream() {
        return die5("executeStream not implemented for node:sqlite");
      }
    };
    const semaphore = yield* make10(1);
    const acquirer = acquireRelease2(succeed5(connection), () => void_3);
    const transactionAcquirer = uninterruptibleMask2((restore) => {
      const fiber2 = getCurrent();
      const scope3 = getUnsafe(fiber2.context, Scope);
      return as2(
        tap2(
          restore(semaphore.take(1)),
          () => addFinalizer2(scope3, semaphore.release(1))
        ),
        connection
      );
    });
    return yield* make18({
      acquirer,
      compiler,
      transactionAcquirer,
      spanAttributes: [["db.system.name", "sqlite"]],
      transformRows
    });
  });
  return effectContext(
    map5(
      makeClient,
      (client) => make2(SqlClient, client)
    )
  ).pipe(provide2(ReactivityNoop));
};

// src/adapters/filesystem-node.ts
import * as NFS from "node:fs/promises";
import * as Path from "node:path";
import * as OS from "node:os";
function errnoToReason(code) {
  switch (code) {
    case "ENOENT":
      return "NotFound";
    case "EACCES":
      return "PermissionDenied";
    case "EEXIST":
      return "AlreadyExists";
    case "EISDIR":
      return "BadResource";
    case "ENOTDIR":
      return "BadResource";
    case "EBUSY":
      return "Busy";
    case "ELOOP":
      return "BadResource";
    case "EPERM":
      return "PermissionDenied";
    default:
      return "Unknown";
  }
}
function tryFs(method, fn2) {
  return tryPromise2({
    try: fn2,
    catch: (err) => {
      const e = err;
      return systemError({
        _tag: errnoToReason(e.code),
        module: "FileSystem",
        method,
        pathOrDescriptor: e.path ?? "",
        description: e.message ?? String(err),
        syscall: e.syscall ?? method
      });
    }
  });
}
var impl = make12({
  access: (path) => tryFs("access", async () => {
    await NFS.access(path);
  }),
  readFile: (path) => tryFs("readFile", async () => new Uint8Array(await NFS.readFile(path))),
  writeFile: (path, data) => tryFs("writeFile", async () => {
    await NFS.writeFile(path, data);
  }),
  stat: (path) => tryFs("stat", async () => {
    const s = await NFS.stat(path);
    return {
      type: s.isDirectory() ? "Directory" : "File",
      size: Size(s.size),
      mtime: some2(s.mtime),
      atime: some2(s.atime),
      birthtime: some2(s.birthtime),
      dev: s.dev,
      ino: some2(s.ino),
      mode: s.mode,
      nlink: some2(s.nlink),
      uid: some2(s.uid),
      gid: some2(s.gid),
      rdev: some2(s.rdev),
      blksize: some2(Size(s.blksize)),
      blocks: some2(s.blocks)
    };
  }),
  remove: (path, opts) => tryFs("remove", async () => {
    await NFS.rm(path, { recursive: true, force: true });
  }),
  makeDirectory: (path, opts) => tryFs("makeDirectory", async () => {
    await NFS.mkdir(path, { recursive: opts?.recursive ?? false });
  }),
  copyFile: (from, to) => tryFs("copyFile", async () => {
    await NFS.copyFile(from, to);
  }),
  copy: (from, to) => tryFs("copy", async () => {
    await NFS.cp(from, to, { recursive: true });
  }),
  readDirectory: (path, opts) => tryFs("readDirectory", async () => await NFS.readdir(path, { recursive: opts?.recursive ?? false })),
  rename: (from, to) => tryFs("rename", async () => {
    await NFS.rename(from, to);
  }),
  truncate: (path, len) => tryFs("truncate", async () => {
    await NFS.truncate(path, Number(len ?? 0));
  }),
  chmod: (path, mode) => tryFs("chmod", async () => {
    await NFS.chmod(path, mode);
  }),
  chown: (path, uid, gid) => tryFs("chown", async () => {
    await NFS.chown(path, uid, gid);
  }),
  utimes: (path, atime, mtime) => tryFs("utimes", async () => {
    await NFS.utimes(path, atime, mtime);
  }),
  link: (from, to) => tryFs("link", async () => {
    await NFS.link(from, to);
  }),
  symlink: (from, to) => tryFs("symlink", async () => {
    await NFS.symlink(from, to);
  }),
  readLink: (path) => tryFs("readLink", async () => await NFS.readlink(path)),
  realPath: (path) => tryFs("realPath", async () => await NFS.realpath(path)),
  makeTempDirectory: (opts) => tryFs("makeTempDirectory", async () => await NFS.mkdtemp(Path.join(opts?.directory ?? OS.tmpdir(), opts?.prefix ?? "rlm-"))),
  makeTempDirectoryScoped: (opts) => acquireRelease2(
    tryFs("makeTempDirectoryScoped", async () => await NFS.mkdtemp(Path.join(opts?.directory ?? OS.tmpdir(), opts?.prefix ?? "rlm-"))),
    (dir) => catchIf2(tryFs("removeTempDirectoryScoped", async () => {
      await NFS.rm(dir, { recursive: true, force: true });
    }), () => true, () => void_3)
  ),
  makeTempFile: (opts) => tryFs("makeTempFile", async () => {
    const dir = await NFS.mkdtemp(Path.join(opts?.directory ?? OS.tmpdir(), opts?.prefix ?? "rlm-"));
    const file = Path.join(dir, `tmpfile${opts?.suffix ?? ""}`);
    await NFS.writeFile(file, "");
    return file;
  }),
  makeTempFileScoped: (opts) => acquireRelease2(
    tryFs("makeTempFileScoped", async () => {
      const dir = await NFS.mkdtemp(Path.join(opts?.directory ?? OS.tmpdir(), opts?.prefix ?? "rlm-"));
      const file = Path.join(dir, `tmpfile${opts?.suffix ?? ""}`);
      await NFS.writeFile(file, "");
      return file;
    }),
    (file) => catchIf2(tryFs("removeTempFileScoped", async () => {
      await NFS.rm(file, { force: true });
    }), () => true, () => void_3)
  ),
  // Not needed for export/import — stub
  open: () => fail5(systemError({
    _tag: "Unknown",
    module: "FileSystem",
    method: "open",
    pathOrDescriptor: "",
    description: "open not implemented in minimal Node adapter",
    syscall: "open"
  })),
  watch: () => {
    throw new Error("FileSystem.watch not implemented in minimal Node adapter");
  }
});
var NodeFileSystemLayer = succeed4(FileSystem, impl);

// src/clone-safe.ts
var DEFAULT_OPTIONS = {
  maxDepth: 10,
  maxArrayItems: 500,
  maxObjectKeys: 500,
  promiseTimeoutMs: 3e4
};
var TIMEOUT = /* @__PURE__ */ Symbol("timeout");
var ABORTED = /* @__PURE__ */ Symbol("aborted");
async function sanitizeForToolPayload(value, options = {}) {
  const state = {
    options: { ...DEFAULT_OPTIONS, ...options },
    warnings: [],
    seen: /* @__PURE__ */ new WeakMap()
  };
  const sanitized = await visit(value, "$", 0, state);
  return { value: sanitized, warnings: state.warnings };
}
function stringifyForToolContent(value) {
  if (value === void 0) return "(void \u2014 side effect only)";
  if (typeof value === "string") return value;
  try {
    const json = JSON.stringify(value, null, 2);
    return json === void 0 ? String(value) : json;
  } catch (err) {
    return JSON.stringify({
      _tag: "StringifyError",
      message: describeError(err),
      fallback: String(value)
    }, null, 2);
  }
}
async function visit(value, path, depth, state) {
  if (state.options.signal?.aborted) {
    state.warnings.push(`Sanitization aborted at ${path}`);
    return { _tag: "SanitizationAborted", path };
  }
  const then = getThen(value);
  if (then) {
    return resolveThenable(value, path, depth, state);
  }
  const primitive = sanitizePrimitive(value);
  if (primitive.handled) return primitive.value;
  if (depth >= state.options.maxDepth) {
    state.warnings.push(`Truncated ${path}: max depth ${state.options.maxDepth} reached`);
    return `[MaxDepth ${state.options.maxDepth} at ${path}]`;
  }
  if (typeof value !== "object" || value === null) return value;
  const obj = value;
  const previousPath = state.seen.get(obj);
  if (previousPath) {
    state.warnings.push(`Replaced circular reference at ${path} (seen at ${previousPath})`);
    return `[Circular ${previousPath}]`;
  }
  state.seen.set(obj, path);
  if (value instanceof Date) {
    return Number.isNaN(value.getTime()) ? "[Invalid Date]" : value.toISOString();
  }
  if (value instanceof RegExp) return String(value);
  if (typeof URL !== "undefined" && value instanceof URL) return value.toString();
  if (value instanceof Error) {
    return sanitizeError(value);
  }
  if (value instanceof WeakMap) return "[WeakMap]";
  if (value instanceof WeakSet) return "[WeakSet]";
  if (value instanceof Map) {
    return sanitizeMap(value, path, depth, state);
  }
  if (value instanceof Set) {
    return sanitizeArray(Array.from(value), path, depth, state, "Set");
  }
  if (Array.isArray(value)) {
    return sanitizeArray(value, path, depth, state, "Array");
  }
  if (value instanceof ArrayBuffer) {
    return { _tag: "ArrayBuffer", byteLength: value.byteLength };
  }
  if (ArrayBuffer.isView(value)) {
    const view = value;
    return {
      _tag: view.constructor?.name ?? "TypedArray",
      byteLength: view.byteLength ?? 0,
      length: view.length ?? void 0
    };
  }
  return sanitizeObject(value, path, depth, state);
}
function sanitizePrimitive(value) {
  switch (typeof value) {
    case "string":
    case "boolean":
    case "undefined":
      return { handled: true, value };
    case "number":
      return { handled: true, value: Number.isFinite(value) ? value : { _tag: "NonFiniteNumber", value: String(value) } };
    case "bigint":
      return { handled: true, value: `${value.toString()}n` };
    case "symbol":
      return { handled: true, value: `[${String(value)}]` };
    case "function":
      return { handled: true, value: `[Function ${value.name || "anonymous"}]` };
    case "object":
      return value === null ? { handled: true, value: null } : { handled: false };
  }
}
function getThen(value) {
  if (typeof value !== "object" && typeof value !== "function" || value === null) return void 0;
  try {
    const then = value.then;
    return typeof then === "function" ? then : void 0;
  } catch {
    return void 0;
  }
}
async function resolveThenable(value, path, depth, state) {
  state.warnings.push(`Resolved unawaited Promise at ${path}; prefer await/Promise.all before returning from mt code`);
  let timer;
  let abortHandler;
  const timeout2 = new Promise((resolve2) => {
    timer = setTimeout(() => resolve2(TIMEOUT), state.options.promiseTimeoutMs);
  });
  const abort = state.options.signal ? new Promise((resolve2) => {
    abortHandler = () => resolve2(ABORTED);
    state.options.signal?.addEventListener("abort", abortHandler, { once: true });
  }) : void 0;
  try {
    const raced = await Promise.race([
      Promise.resolve(value),
      timeout2,
      ...abort ? [abort] : []
    ]);
    if (raced === TIMEOUT) {
      state.warnings.push(`Promise at ${path} did not settle within ${state.options.promiseTimeoutMs}ms`);
      return { _tag: "UnresolvedPromise", path, timeoutMs: state.options.promiseTimeoutMs };
    }
    if (raced === ABORTED) {
      state.warnings.push(`Promise at ${path} aborted by tool cancellation`);
      return { _tag: "AbortedPromise", path };
    }
    return visit(raced, path, depth, state);
  } catch (err) {
    state.warnings.push(`Promise at ${path} rejected: ${describeError(err)}`);
    return { _tag: "RejectedPromise", path, message: describeError(err) };
  } finally {
    if (timer) clearTimeout(timer);
    if (abortHandler) state.options.signal?.removeEventListener("abort", abortHandler);
  }
}
async function sanitizeArray(value, path, depth, state, tag2) {
  const max = state.options.maxArrayItems;
  const slice = value.slice(0, max);
  const out = await Promise.all(
    slice.map((item, i) => visit(item, `${path}[${i}]`, depth + 1, state))
  );
  if (value.length > max) {
    state.warnings.push(`Truncated ${tag2} at ${path}: ${value.length - max} item(s) omitted`);
    out.push(`[${tag2} truncated: ${value.length - max} more item(s)]`);
  }
  return out;
}
async function sanitizeMap(value, path, depth, state) {
  const max = state.options.maxArrayItems;
  const rawEntries = Array.from(value.entries()).slice(0, max);
  const entries = await Promise.all(
    rawEntries.map(async ([key, val], i) => [
      await visit(key, `${path}.<key:${i}>`, depth + 1, state),
      await visit(val, `${path}.<value:${i}>`, depth + 1, state)
    ])
  );
  if (value.size > max) {
    state.warnings.push(`Truncated Map at ${path}: ${value.size - max} entries omitted`);
    entries.push(`[Map truncated: ${value.size - max} more entries]`);
  }
  return { _tag: "Map", entries };
}
async function sanitizeObject(value, path, depth, state) {
  const out = {};
  const proto = Object.getPrototypeOf(value);
  const className = proto && proto !== Object.prototype && proto.constructor?.name;
  if (className && className !== "Object") {
    out._class = className;
  }
  let keys;
  try {
    keys = Object.keys(value);
  } catch (err) {
    return { _tag: "UnreadableObject", message: describeError(err) };
  }
  const max = state.options.maxObjectKeys;
  const entries = await Promise.all(
    keys.slice(0, max).map(async (key) => {
      const childPath = `${path}.${formatKey(key)}`;
      try {
        return [key, await visit(value[key], childPath, depth + 1, state)];
      } catch (err) {
        state.warnings.push(`Could not read ${childPath}: ${describeError(err)}`);
        return [key, { _tag: "UnreadableProperty", message: describeError(err) }];
      }
    })
  );
  for (const [key, entryValue] of entries) out[key] = entryValue;
  if (keys.length > max) {
    state.warnings.push(`Truncated object at ${path}: ${keys.length - max} key(s) omitted`);
    out.__truncated__ = `${keys.length - max} more key(s)`;
  }
  const symbolKeys = Object.getOwnPropertySymbols(value);
  if (symbolKeys.length > 0) {
    state.warnings.push(`Dropped ${symbolKeys.length} symbol key(s) at ${path}`);
  }
  return out;
}
function sanitizeError(error) {
  const out = {
    _tag: "Error",
    name: error.name,
    message: error.message
  };
  if (error.stack) out.stack = error.stack;
  if ("cause" in error) out.cause = String(error.cause);
  return out;
}
function formatKey(key) {
  return /^[A-Za-z_$][\w$]*$/.test(key) ? key : JSON.stringify(key);
}
function describeError(err) {
  return err instanceof Error ? err.message : String(err);
}
export {
  NodeFileSystemLayer,
  createMetatool,
  sanitizeForToolPayload,
  layer as sqliteNodeLayer,
  stringifyForToolContent
};
