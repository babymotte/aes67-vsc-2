/* @refresh reload */
import { render } from "solid-js/web";
import "./index.css";
import { Route, Router } from "@solidjs/router";
import App from "./App";
import { Worterbuch } from "./worterbuch";

const wrapper = document.getElementById("root");

if (!wrapper) {
  throw new Error("Wrapper div not found");
}

render(
  () => (
    <>
      <Worterbuch />
      <Router>
        <Route path="/rx" component={() => <App tab={1} />} />
        <Route path="/config" component={() => <App tab={2} />} />
        <Route path="*" component={() => <App tab={0} />} />
      </Router>
    </>
  ),
  wrapper
);
