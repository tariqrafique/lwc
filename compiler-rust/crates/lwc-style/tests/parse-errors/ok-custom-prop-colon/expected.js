function stylesheet(token, useActualHostSelector, useNativeDirPseudoclass) {
  var shadowSelector = token ? ("[" + token + "]") : "";
  var hostSelector = token ? ("[" + token + "-host]") : "";
  var suffixToken = token ? ("-" + token) : "";
  return "a" + shadowSelector + "{--x: a:b}";
  /*LWC compiler vX.X.X*/
}
export default [stylesheet];