<!-- You better to read this file in review mode -->
*This file is translate of [CONTRIBUTING_RU.md](CONTRIBUTING_RU.md)*
# Contributing to W16

First of all, **thank you for your interest in W16**! Developer contributions help improve the runtime, making it much better.

However, before you start submitting pull requests to the project, please read the guidelines below.

## Contribution Guidelines
1. **Make sure you have a compatible version**:
    * `rustc` **1.93.1**
    * `cargo` **1.93.1**

2. **Before submitting a pull request, your version of the project must pass all tests**. Run the tests in the root directory of the workspace:
    ```bash
    cargo test
    ```

3. **Commit Messaging**. Please use descriptive prefixes in your commit messages
to make it clear which part of the runtime is affected by the changes:
```text
feat(...):     ... — adding a new feature to some part of the project.
fix(...):      ... — a fix.
doc(...):      ... — adding or updating documentation.
some(...):     ... — adding or changing some things in project(small changes).
refactor(...): ... — changing the logic or structure of the project.
```

## How to clone the project locally
```bash
# Use HTTPS or SSH, whichever you prefer

# Via HTTPS
git clone https://github.com/dev-er1/w16-project.git

# Via SSH
git clone git@github.com:dev-er1/w16-project.git

cd w16-project
```

## Code and coding style guidelines
- **Comments**: It is preferable to write comments in Russian, but you may write in any language.
- **Coding style**: Write however you like.