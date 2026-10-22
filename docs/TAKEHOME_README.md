# backend-takehome

> **This is the original starter README, kept for reference.** The `GET /` hello-world route
> it describes was replaced by the two assignment routes, so the `curl` below no longer
> applies — see [README.md](README.md) for how to configure and run the submission.

Install rust: https://www.rust-lang.org/learn/get-started

To run the project, after installing rust, run 

`$ cargo run`

In same directory as the readme. 

To verify the project is working, once it's running, you should be able to

`$ curl http://localhost:3000` and see `{"data":"hello world"}`

Useful references:
Axum documentation: https://docs.rs/axum/latest/axum/all.html#
Serde documentation: https://serde.rs/
Env vars in rust (please do not commmit environment variables to github): https://doc.rust-lang.org/book/ch12-05-working-with-environment-variables.html

