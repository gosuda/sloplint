import { render } from "./service.js";

test("renders", () => {
  if (render("a") !== "a") {
    throw new Error("mismatch");
  }
});
