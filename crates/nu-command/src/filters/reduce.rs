use std::sync::atomic::Ordering;

use nu_engine::{eval_block, CallExt};

use nu_protocol::ast::Call;
use nu_protocol::engine::{CaptureBlock, Command, EngineState, Stack};
use nu_protocol::{
    Example, IntoPipelineData, PipelineData, ShellError, Signature, Span, SyntaxShape, Value,
};

#[derive(Clone)]
pub struct Reduce;

impl Command for Reduce {
    fn name(&self) -> &str {
        "reduce"
    }

    fn signature(&self) -> Signature {
        Signature::build("reduce")
            .named(
                "fold",
                SyntaxShape::Any,
                "reduce with initial value",
                Some('f'),
            )
            .required(
                "block",
                SyntaxShape::Block(Some(vec![SyntaxShape::Any, SyntaxShape::Any])),
                "reducing function",
            )
            .switch("numbered", "iterate with an index", Some('n'))
    }

    fn usage(&self) -> &str {
        "Aggregate a list table to a single value using an accumulator block."
    }

    fn search_terms(&self) -> Vec<&str> {
        vec!["map", "fold", "foldl"]
    }

    fn examples(&self) -> Vec<Example> {
        vec![
            Example {
                example: "[ 1 2 3 4 ] | reduce {|it, acc| $it + $acc }",
                description: "Sum values of a list (same as 'math sum')",
                result: Some(Value::Int {
                    val: 10,
                    span: Span::test_data(),
                }),
            },
            Example {
                example: "[ 1 2 3 ] | reduce -n {|it, acc| $acc + $it.item }",
                description: "Sum values of a list (same as 'math sum')",
                result: Some(Value::Int {
                    val: 6,
                    span: Span::test_data(),
                }),
            },
            Example {
                example: "[ 1 2 3 4 ] | reduce -f 10 {|it, acc| $acc + $it }",
                description: "Sum values with a starting value (fold)",
                result: Some(Value::Int {
                    val: 20,
                    span: Span::test_data(),
                }),
            },
            Example {
                example: r#"[ i o t ] | reduce -f "Arthur, King of the Britons" {|it, acc| $acc | str replace -a $it "X" }"#,
                description: "Replace selected characters in a string with 'X'",
                result: Some(Value::String {
                    val: "ArXhur, KXng Xf Xhe BrXXXns".to_string(),
                    span: Span::test_data(),
                }),
            },
            Example {
                example: r#"[ one longest three bar ] | reduce -n { |it, acc|
                    if ($it.item | str length) > ($acc | str length) {
                        $it.item
                    } else {
                        $acc
                    }
                }"#,
                description: "Find the longest string and its index",
                result: Some(Value::String {
                    val: "longest".to_string(),
                    span: Span::test_data(),
                }),
            },
        ]
    }

    fn run(
        &self,
        engine_state: &EngineState,
        stack: &mut Stack,
        call: &Call,
        input: PipelineData,
    ) -> Result<PipelineData, ShellError> {
        let span = call.head;

        let fold: Option<Value> = call.get_flag(engine_state, stack, "fold")?;
        let numbered = call.has_flag("numbered");
        let capture_block: CaptureBlock = call.req(engine_state, stack, 0)?;
        let mut stack = stack.captures_to_stack(&capture_block.captures);
        let block = engine_state.get_block(capture_block.block_id);
        let ctrlc = engine_state.ctrlc.clone();

        let orig_env_vars = stack.env_vars.clone();
        let orig_env_hidden = stack.env_hidden.clone();

        let redirect_stdout = call.redirect_stdout;
        let redirect_stderr = call.redirect_stderr;

        let mut input_iter = input.into_iter();

        let (off, start_val) = if let Some(val) = fold {
            (0, val)
        } else if let Some(val) = input_iter.next() {
            (1, val)
        } else {
            return Err(ShellError::SpannedLabeledError(
                "Expected input".to_string(),
                "needs input".to_string(),
                span,
            ));
        };

        let mut acc = start_val;

        for (idx, x) in input_iter.enumerate() {
            stack.with_env(&orig_env_vars, &orig_env_hidden);
            // if the acc coming from previous iter is indexed, drop the index
            acc = if let Value::Record { cols, vals, .. } = &acc {
                if cols.len() == 2 && vals.len() == 2 {
                    if cols[0].eq("index") && cols[1].eq("item") {
                        vals[1].clone()
                    } else {
                        acc
                    }
                } else {
                    acc
                }
            } else {
                acc
            };

            if let Some(var) = block.signature.get_positional(0) {
                if let Some(var_id) = &var.var_id {
                    let it = if numbered {
                        Value::Record {
                            cols: vec!["index".to_string(), "item".to_string()],
                            vals: vec![
                                Value::Int {
                                    val: idx as i64 + off,
                                    span,
                                },
                                x,
                            ],
                            span,
                        }
                    } else {
                        x
                    };

                    stack.add_var(*var_id, it);
                }
            }
            if let Some(var) = block.signature.get_positional(1) {
                if let Some(var_id) = &var.var_id {
                    stack.add_var(*var_id, acc);
                }
            }

            acc = eval_block(
                engine_state,
                &mut stack,
                block,
                PipelineData::new(span),
                redirect_stdout,
                redirect_stderr,
            )?
            .into_value(span);

            if let Some(ctrlc) = &ctrlc {
                if ctrlc.load(Ordering::SeqCst) {
                    break;
                }
            }
        }

        Ok(acc.with_span(span).into_pipeline_data())
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn test_examples() {
        use crate::test_examples;

        test_examples(Reduce {})
    }
}
