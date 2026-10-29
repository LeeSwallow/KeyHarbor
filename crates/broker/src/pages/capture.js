// Runs in KeyHarbor's isolated world of every frame (08 §8.2 ②). The page cannot see or call
// the binding, and only trusted submits count: a page script that submits a form itself
// (requestSubmit, synthetic events) reports nothing.
(() => {
  const send = globalThis.__khCapture;
  if (typeof send !== "function" || globalThis.__khCaptureInstalled) return;
  globalThis.__khCaptureInstalled = true;
  const USER_TYPES = ["text", "email", "tel", ""];
  document.addEventListener(
    "submit",
    (event) => {
      if (!event.isTrusted || !(event.target instanceof HTMLFormElement)) return;
      const inputs = Array.from(event.target.querySelectorAll("input"));
      const password = inputs.find((i) => i.type === "password" && i.value);
      if (!password) return;
      const username = inputs
        .slice(0, inputs.indexOf(password))
        .reverse()
        .find((i) => USER_TYPES.includes(i.type) && i.value);
      send(
        JSON.stringify({
          origin: location.origin,
          username: username ? username.value : null,
          password: password.value,
        }),
      );
    },
    true,
  );
})();
