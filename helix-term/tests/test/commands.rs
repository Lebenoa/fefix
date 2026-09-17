use helix_term::application::Application;
use tempfile::tempdir;

use super::*;

mod insert;
mod movement;
mod reverse_selection_contents;
mod rotate_selection_contents;
mod write;

#[tokio::test(flavor = "multi_thread")]
async fn search_selection_detect_word_boundaries_at_eof() -> anyhow::Result<()> {
    // <https://github.com/helix-editor/helix/issues/12609>
    test((
        indoc! {"\
            #[o|]#ne
            two
            three"},
        "gej*h",
        indoc! {"\
            one
            two
            three#[
            |]#"},
    ))
    .await?;

    Ok(())
}

#[tokio::test(flavor = "multi_thread")]
async fn test_selection_duplication() -> anyhow::Result<()> {
    // Forward
    test((
        indoc! {"\
            #[lo|]#rem
            ipsum
            dolor
            "},
        "CC",
        indoc! {"\
            #(lo|)#rem
            #(ip|)#sum
            #[do|]#lor
            "},
    ))
    .await?;

    // Backward
    test((
        indoc! {"\
            #[|lo]#rem
            ipsum
            dolor
            "},
        "CC",
        indoc! {"\
            #(|lo)#rem
            #(|ip)#sum
            #[|do]#lor
            "},
    ))
    .await?;

    // Copy the selection to previous line, skipping the first line in the file
    test((
        indoc! {"\
            test
            #[testitem|]#
            "},
        "<A-C>",
        indoc! {"\
            test
            #[testitem|]#
            "},
    ))
    .await?;

    // Copy the selection to previous line, including the first line in the file
    test((
        indoc! {"\
            test
            #[test|]#
            "},
        "<A-C>",
        indoc! {"\
            #[test|]#
            #(test|)#
            "},
    ))
    .await?;

    // Copy the selection to next line, skipping the last line in the file
    test((
        indoc! {"\
            #[testitem|]#
            test
            "},
        "C",
        indoc! {"\
            #[testitem|]#
            test
            "},
    ))
    .await?;

    // Copy the selection to next line, including the last line in the file
    test((
        indoc! {"\
            #[test|]#
            test
            "},
        "C",
        indoc! {"\
            #(test|)#
            #[test|]#
            "},
    ))
    .await?;
    Ok(())
}

#[tokio::test(flavor = "multi_thread")]
async fn test_goto_file_impl() -> anyhow::Result<()> {
    let file = tempfile::NamedTempFile::new()?;

    fn match_paths(app: &Application, matches: Vec<&str>) -> usize {
        app.editor
            .documents()
            .filter_map(|d| d.path()?.file_name())
            .filter(|n| matches.iter().any(|m| *m == n.to_string_lossy()))
            .count()
    }

    // Single selection
    test_key_sequence(
        &mut AppBuilder::new().with_file(file.path(), None).build()?,
        Some("ione.js<esc>%gf"),
        Some(&|app| {
            assert_eq!(1, match_paths(app, vec!["one.js"]));
        }),
        false,
    )
    .await?;

    // Multiple selection
    test_key_sequence(
        &mut AppBuilder::new().with_file(file.path(), None).build()?,
        Some("ione.js<ret>two.js<esc>%<A-s>gf"),
        Some(&|app| {
            assert_eq!(2, match_paths(app, vec!["one.js", "two.js"]));
        }),
        false,
    )
    .await?;

    // Cursor on first quote
    test_key_sequence(
        &mut AppBuilder::new().with_file(file.path(), None).build()?,
        Some("iimport 'one.js'<esc>B;gf"),
        Some(&|app| {
            assert_eq!(1, match_paths(app, vec!["one.js"]));
        }),
        false,
    )
    .await?;

    // Cursor on last quote
    test_key_sequence(
        &mut AppBuilder::new().with_file(file.path(), None).build()?,
        Some("iimport 'one.js'<esc>bgf"),
        Some(&|app| {
            assert_eq!(1, match_paths(app, vec!["one.js"]));
        }),
        false,
    )
    .await?;

    // ';' is behind the path
    test_key_sequence(
        &mut AppBuilder::new().with_file(file.path(), None).build()?,
        Some("iimport 'one.js';<esc>B;gf"),
        Some(&|app| {
            assert_eq!(1, match_paths(app, vec!["one.js"]));
        }),
        false,
    )
    .await?;

    // allow numeric values in path
    test_key_sequence(
        &mut AppBuilder::new().with_file(file.path(), None).build()?,
        Some("iimport 'one123.js'<esc>B;gf"),
        Some(&|app| {
            assert_eq!(1, match_paths(app, vec!["one123.js"]));
        }),
        false,
    )
    .await?;

    Ok(())
}

#[tokio::test(flavor = "multi_thread")]
async fn test_multi_selection_paste() -> anyhow::Result<()> {
    test((
        indoc! {"\
            #[|lorem]#
            #(|ipsum)#
            #(|dolor)#
            "},
        "yp",
        indoc! {"\
            lorem#[|lorem]#
            ipsum#(|ipsum)#
            dolor#(|dolor)#
            "},
    ))
    .await?;

    Ok(())
}

#[tokio::test(flavor = "multi_thread")]
async fn test_multi_selection_shell_commands() -> anyhow::Result<()> {
    // pipe
    test((
        indoc! {"\
            #[|lorem]#
            #(|ipsum)#
            #(|dolor)#
            "},
        "|echo foo<ret>",
        indoc! {"\
            #[|foo]#
            #(|foo)#
            #(|foo)#"
        },
    ))
    .await?;

    // insert-output
    test((
        indoc! {"\
            #[|lorem]#
            #(|ipsum)#
            #(|dolor)#
            "},
        "!echo foo<ret>",
        indoc! {"\
            #[|foo]#lorem
            #(|foo)#ipsum
            #(|foo)#dolor
            "},
    ))
    .await?;

    // append-output
    test((
        indoc! {"\
            #[|lorem]#
            #(|ipsum)#
            #(|dolor)#
            "},
        "<A-!>echo foo<ret>",
        indoc! {"\
            lorem#[|foo]#
            ipsum#(|foo)#
            dolor#(|foo)#
            "},
    ))
    .await?;

    Ok(())
}

#[tokio::test(flavor = "multi_thread")]
async fn test_undo_redo() -> anyhow::Result<()> {
    // A jumplist selection is created at a point which is undone.
    //
    // * 2[<space>   Add two newlines at line start. We're now on line 3.
    // * <C-s>       Save the selection on line 3 in the jumplist.
    // * u           Undo the two newlines. We're now on line 1.
    // * <C-o><C-i>  Jump forward an back again in the jumplist. This would panic
    //               if the jumplist were not being updated correctly.
    test((
        "#[|]#",
        "2[<space><C-s>u<C-o><C-i>",
        "#[|]#",
        LineFeedHandling::AsIs,
    ))
    .await?;

    // A jumplist selection is passed through an edit and then an undo and then a redo.
    //
    // * [<space>    Add a newline at line start. We're now on line 2.
    // * <C-s>       Save the selection on line 2 in the jumplist.
    // * kd          Delete line 1. The jumplist selection should be adjusted to the new line 1.
    // * uU          Undo and redo the `kd` edit.
    // * <C-o>       Jump back in the jumplist. This would panic if the jumplist were not being
    //               updated correctly.
    // * <C-i>       Jump forward to line 1.
    test((
        "#[|]#",
        "[<space><C-s>kduU<C-o><C-i>",
        "#[|]#",
        LineFeedHandling::AsIs,
    ))
    .await?;

    // In this case we 'redo' manually to ensure that the transactions are composing correctly.
    test((
        "#[|]#",
        "[<space>u[<space>u",
        "#[|]#",
        LineFeedHandling::AsIs,
    ))
    .await?;

    Ok(())
}

#[tokio::test(flavor = "multi_thread")]
async fn test_extend_line() -> anyhow::Result<()> {
    // extend with line selected then count
    test((
        indoc! {"\
            #[l|]#orem
            ipsum
            dolor
            
            "},
        "x2x",
        indoc! {"\
            #[lorem
            ipsum
            dolor\n|]#
            
            "},
    ))
    .await?;

    // extend with count on partial selection
    test((
        indoc! {"\
            #[l|]#orem
            ipsum
            
            "},
        "2x",
        indoc! {"\
            #[lorem
            ipsum\n|]#
            
            "},
    ))
    .await?;

    Ok(())
}

#[tokio::test(flavor = "multi_thread")]
async fn test_character_info() -> anyhow::Result<()> {
    // UTF-8, single byte
    test_key_sequence(
        &mut helpers::AppBuilder::new().build()?,
        Some("ih<esc>h:char<ret>"),
        Some(&|app| {
            assert_eq!(
                r#""h" (U+0068) Dec 104 Hex 68"#,
                app.editor.get_status().unwrap().0
            );
        }),
        false,
    )
    .await?;

    // UTF-8, multi-byte
    test_key_sequence(
        &mut helpers::AppBuilder::new().build()?,
        Some("ië<esc>h:char<ret>"),
        Some(&|app| {
            assert_eq!(
                r#""ë" (U+0065 U+0308) Hex 65 + cc 88"#,
                app.editor.get_status().unwrap().0
            );
        }),
        false,
    )
    .await?;

    // Multiple characters displayed as one, escaped characters
    test_key_sequence(
        &mut helpers::AppBuilder::new().build()?,
        Some(":line<minus>ending crlf<ret>:char<ret>"),
        Some(&|app| {
            assert_eq!(
                r#""\r\n" (U+000d U+000a) Hex 0d + 0a"#,
                app.editor.get_status().unwrap().0
            );
        }),
        false,
    )
    .await?;

    // Non-UTF-8
    test_key_sequence(
        &mut helpers::AppBuilder::new().build()?,
        Some(":encoding ascii<ret>ih<esc>h:char<ret>"),
        Some(&|app| {
            assert_eq!(r#""h" Dec 104 Hex 68"#, app.editor.get_status().unwrap().0);
        }),
        false,
    )
    .await?;

    Ok(())
}

#[tokio::test(flavor = "multi_thread")]
async fn test_delete_char_backward() -> anyhow::Result<()> {
    // don't panic when deleting overlapping ranges
    test(("#(x|)# #[x|]#", "c<space><backspace><esc>", "#[\n|]#")).await?;
    test((
        "#( |)##( |)#a#( |)#axx#[x|]#a",
        "li<backspace><esc>",
        "#(a|)##(|a)#xx#[|a]#",
    ))
    .await?;

    Ok(())
}

// Cursor behavior is different when the text is created in the buffer vs loaded from a file.
// This test will not work for reproducing the crash or verifying the result after the fix.
// // #[tokio::test(flavor = "multi_thread")]
// async fn test_try_restore_indent() -> anyhow::Result<()> {
//     test((" #[ |]#foo\na#( |)#bar\n", "o<C-u><esc>", " foo\n#[\n|]#a bar\n#(\n|)#")).await?;
//     Ok(())
// }

#[tokio::test(flavor = "multi_thread")]
async fn test_try_restore_indent() -> anyhow::Result<()> {
    // Bug: 15228 try_restore_indent uses primary cursor position for all selections,
    // causing invalid range errors when multiple cursors are on different lines
    let file = temp_file_with_contents("  foo\na bar\n")?;
    test_key_sequence(
        &mut AppBuilder::new().with_file(file.path(), None).build()?,
        Some("jl<A-C>o<C-u><esc>"),
        None,
        false,
    )
    .await?;

    Ok(())
}

#[tokio::test(flavor = "multi_thread")]
async fn test_delete_word_backward() -> anyhow::Result<()> {
    // don't panic when deleting overlapping ranges
    test(("fo#[o|]#ba#(r|)#", "a<C-w><esc>", "#[\n|]#")).await?;
    Ok(())
}

#[tokio::test(flavor = "multi_thread")]
async fn test_delete_word_forward() -> anyhow::Result<()> {
    // don't panic when deleting overlapping ranges
    test(("fo#[o|]#b#(|ar)#", "i<A-d><esc>", "fo#[\n|]#")).await?;
    Ok(())
}

#[tokio::test(flavor = "multi_thread")]
async fn test_delete_char_forward() -> anyhow::Result<()> {
    test((
        indoc! {"\
                #[abc|]#def
                #(abc|)#ef
                #(abc|)#f
                #(abc|)#
            "},
        "a<del><esc>",
        indoc! {"\
                #[abc|]#ef
                #(abc|)#f
                #(abc|)#
                #(abc|)#
            "},
    ))
    .await?;

    Ok(())
}

#[tokio::test(flavor = "multi_thread")]
async fn test_insert_with_indent() -> anyhow::Result<()> {
    const INPUT: &str = indoc! { "
        #[f|]#n foo() {
            if let Some(_) = None {

            }
         
        }

        fn bar() {

        }
        "
    };

    // insert_at_line_start
    test((
        INPUT,
        ":lang rust<ret>%<A-s>I",
        indoc! { "
            #[f|]#n foo() {
                #(i|)#f let Some(_) = None {
                    #(\n|)#
                #(}|)#
            #( |)#
            #(}|)#
            #(\n|)#
            #(f|)#n bar() {
                #(\n|)#
            #(}|)#
            "
        },
    ))
    .await?;

    // insert_at_line_end
    test((
        INPUT,
        ":lang rust<ret>%<A-s>A",
        indoc! { "
            fn foo() {#[\n|]#
                if let Some(_) = None {#(\n|)#
                    #(\n|)#
                }#(\n|)#
             #(\n|)#
            }#(\n|)#
            #(\n|)#
            fn bar() {#(\n|)#
                #(\n|)#
            }#(\n|)#
            "
        },
    ))
    .await?;

    Ok(())
}

#[tokio::test(flavor = "multi_thread")]
async fn test_join_selections() -> anyhow::Result<()> {
    // normal join
    test((
        indoc! {"\
            #[a|]#bc
            def
        "},
        "J",
        indoc! {"\
            #[a|]#bc def
        "},
    ))
    .await?;

    // join with empty line
    test((
        indoc! {"\
            #[a|]#bc

            def
        "},
        "JJ",
        indoc! {"\
            #[a|]#bc def
        "},
    ))
    .await?;

    // join with additional space in non-empty line
    test((
        indoc! {"\
            #[a|]#bc

                def
        "},
        "JJ",
        indoc! {"\
            #[a|]#bc def
        "},
    ))
    .await?;

    Ok(())
}

#[tokio::test(flavor = "multi_thread")]
async fn test_join_selections_space() -> anyhow::Result<()> {
    // join with empty lines panic
    test((
        indoc! {"\
            #[a

            b

            c

            d

            e|]#
        "},
        "<A-J>",
        indoc! {"\
            a#[ |]#b#( |)#c#( |)#d#( |)#e
        "},
    ))
    .await?;

    // normal join
    test((
        indoc! {"\
            #[a|]#bc
            def
        "},
        "<A-J>",
        indoc! {"\
            abc#[ |]#def
        "},
    ))
    .await?;

    // join with empty line
    test((
        indoc! {"\
            #[a|]#bc

            def
        "},
        "<A-J>",
        indoc! {"\
            #[a|]#bc
            def
        "},
    ))
    .await?;

    // join with additional space in non-empty line
    test((
        indoc! {"\
            #[a|]#bc

                def
        "},
        "<A-J><A-J>",
        indoc! {"\
            abc#[ |]#def
        "},
    ))
    .await?;

    // join with retained trailing spaces
    test((
        indoc! {"\
            #[aaa   

            bb  

            c |]#
        "},
        "<A-J>",
        indoc! {"\
            aaa   #[ |]#bb  #( |)#c 
        "},
    ))
    .await?;

    Ok(())
}

#[tokio::test(flavor = "multi_thread")]
async fn test_join_selections_comment() -> anyhow::Result<()> {
    test((
        indoc! {"\
            /// #[a|]#bc
            /// def
        "},
        ":lang rust<ret>J",
        indoc! {"\
            /// #[a|]#bc def
        "},
    ))
    .await?;

    // Only join if the comment token matches the previous line.
    test((
        indoc! {"\
            #[| // a
            // b
            /// c
            /// d
            e
            /// f
            // g]#
        "},
        ":lang rust<ret>J",
        indoc! {"\
            #[| // a b /// c d e f // g]#
        "},
    ))
    .await?;

    test((
        "#[|\t// Join comments
\t// with indent]#",
        ":lang go<ret>J",
        "#[|\t// Join comments with indent]#",
    ))
    .await?;

    Ok(())
}

#[tokio::test(flavor = "multi_thread")]
async fn test_toggle_comments_inside_comment_injection() -> anyhow::Result<()> {
    // A `//` line comment's text is injected as the `comment` language, which has no
    // comment-tokens of its own. With the cursor inside the comment, toggling must
    // resolve tokens from the enclosing language and un-comment the line,
    // not fall back to the hardcoded default `#`.
    test((
        indoc! {"\
            // #[a|]#bc
        "},
        ":lang rust<ret><C-c>",
        indoc! {"\
            #[a|]#bc
        "},
    ))
    .await?;

    // A `///` doc comment's text is injected as markdown (no line comment token of
    // its own). Toggling must strip the whole `///` marker via Rust's tokens rather
    // than insert a markdown `<!-- -->` inside or leave a stray `/`.
    test((
        indoc! {"\
            /// #[a|]#bc
        "},
        ":lang rust<ret><C-c>",
        indoc! {"\
            #[a|]#bc
        "},
    ))
    .await?;

    // Likewise for the `//!` inner doc comment marker.
    test((
        indoc! {"\
            //! #[a|]#bc
        "},
        ":lang rust<ret><C-c>",
        indoc! {"\
            #[a|]#bc
        "},
    ))
    .await?;

    // Commenting a normal code line still uses the top-level language's token
    // (no injection layer at the cursor), no regression for the common case.
    test((
        indoc! {"\
            #[l|]#et x = 5;
        "},
        ":lang rust<ret><C-c>",
        indoc! {"\
            // #[l|]#et x = 5;
        "},
    ))
    .await?;

    Ok(())
}

#[tokio::test(flavor = "multi_thread")]
async fn test_read_file() -> anyhow::Result<()> {
    let mut file = tempfile::NamedTempFile::new()?;
    let contents_to_read = "some contents";
    let output_file = helpers::temp_file_with_contents(contents_to_read)?;

    test_key_sequence(
        &mut helpers::AppBuilder::new()
            .with_file(file.path(), None)
            .build()?,
        Some(&format!(":r {:?}<ret><esc>:w<ret>", output_file.path())),
        Some(&|app| {
            assert!(!app.editor.is_err(), "error: {:?}", app.editor.get_status());
        }),
        false,
    )
    .await?;

    let expected_contents = LineFeedHandling::Native.apply(contents_to_read);
    helpers::assert_file_has_content(&mut file, &expected_contents)?;

    Ok(())
}

#[tokio::test(flavor = "multi_thread")]
async fn surround_delete() -> anyhow::Result<()> {
    // Test `surround_delete` when head < anchor
    test(("(#[|  ]#)", "mdm", "#[|  ]#")).await?;
    test(("(#[|  ]#)", "md(", "#[|  ]#")).await?;

    Ok(())
}

#[tokio::test(flavor = "multi_thread")]
async fn surround_replace_ts() -> anyhow::Result<()> {
    const INPUT: &str = r#"\
fn foo() {
    if let Some(_) = None {
        testing!("f#[|o]#o)");
    }
}
"#;
    test((
        INPUT,
        ":lang rust<ret>mrm'",
        r#"\
fn foo() {
    if let Some(_) = None {
        testing!('f#[|o]#o)');
    }
}
"#,
    ))
    .await?;

    test((
        INPUT,
        ":lang rust<ret>3mrm[",
        r#"\
fn foo() {
    if let Some(_) = None [
        testing!("f#[|o]#o)");
    ]
}
"#,
    ))
    .await?;

    test((
        INPUT,
        ":lang rust<ret>2mrm{",
        r#"\
fn foo() {
    if let Some(_) = None {
        testing!{"f#[|o]#o)"};
    }
}
"#,
    ))
    .await?;

    test((
        indoc! {"\
            #[a
            b
            c
            d
            e|]#
            f
            "},
        "s\\n<ret>r,",
        "a#[,|]#b#(,|)#c#(,|)#d#(,|)#e\nf\n",
    ))
    .await?;

    Ok(())
}

#[tokio::test(flavor = "multi_thread")]
async fn macro_play_within_macro_record() -> anyhow::Result<()> {
    // <https://github.com/helix-editor/helix/issues/12697>
    //
    // * `"aQihello<esc>Q` record a macro to register 'a' which inserts "hello"
    // * `Q"aq<space>world<esc>Q` record a macro to the default macro register which plays the
    //   macro in register 'a' and then inserts " world"
    // * `%d` clear the buffer
    // * `q` replay the macro in the default macro register
    // * `i<ret>` add a newline at the end
    //
    // The inner macro in register 'a' should replay within the outer macro exactly once to insert
    // "hello world".
    test((
        indoc! {"\
            #[|]#
        "},
        r#""aQihello<esc>QQ"aqi<space>world<esc>Q%dqi<ret>"#,
        indoc! {"\
            hello world
            #[|]#"},
    ))
    .await?;

    Ok(())
}

#[tokio::test(flavor = "multi_thread")]
async fn global_search_with_multibyte_chars() -> anyhow::Result<()> {
    // Assert that `helix_term::commands::global_search` handles multibyte characters correctly.
    test((
        indoc! {"\
            // Hello world!
            // #[|
            ]#
            "},
        // start global search
        " /«十分に長い マルチバイトキャラクター列» で検索<ret><esc>",
        indoc! {"\
            // Hello world!
            // #[|
            ]#
            "},
    ))
    .await?;

    Ok(())
}

#[tokio::test(flavor = "multi_thread")]
async fn directory_argument_opens_tree_when_enabled() -> anyhow::Result<()> {
    // `ffx <directory>` (e.g. `ffx .`) shows the directory in the persistent
    // file tree window when `[editor.file-tree] enable = true` is set.
    let dir = tempdir()?;
    let mut config = test_config();
    config.editor.file_tree.enable = true;
    let app = AppBuilder::new()
        .with_file(dir.path().to_path_buf(), Some(Default::default()))
        .with_config(config)
        .build()?;
    assert!(app.editor.file_tree_window.open);
    Ok(())
}

#[tokio::test(flavor = "multi_thread")]
async fn directory_argument_keeps_picker_in_default_mode() -> anyhow::Result<()> {
    let dir = tempdir()?;
    let app = AppBuilder::new()
        .with_file(dir.path().to_path_buf(), Some(Default::default()))
        .build()?;
    assert!(!app.editor.file_tree_window.open);
    Ok(())
}

#[tokio::test(flavor = "multi_thread")]
async fn file_explorer_toggles_tree_when_enabled() -> anyhow::Result<()> {
    // With `[editor.file-tree] enable = true`, `<space>e` opens the tree
    // window on the first press and closes it on the second (it toggles).
    let mut config = test_config();
    config.editor.file_tree.enable = true;
    let mut app = AppBuilder::new().with_config(config).build()?;
    assert!(!app.editor.file_tree_window.open);
    test_key_sequences(
        &mut app,
        vec![
            (
                Some("<space>e"),
                Some(&|app: &Application| {
                    assert!(app.editor.file_tree_window.open);
                }),
            ),
            (
                Some("<esc><space>e"),
                Some(&|app: &Application| {
                    assert!(!app.editor.file_tree_window.open);
                }),
            ),
        ],
        false,
    )
    .await
}

#[tokio::test(flavor = "multi_thread")]
async fn q_closes_tree_while_focused() -> anyhow::Result<()> {
    // With the tree window focused, `q` closes it outright.
    let mut config = test_config();
    config.editor.file_tree.enable = true;
    let mut app = AppBuilder::new().with_config(config).build()?;
    assert!(!app.editor.file_tree_window.open);
    test_key_sequences(
        &mut app,
        vec![
            (
                Some("<space>e"),
                Some(&|app: &Application| {
                    assert!(app.editor.file_tree_window.open);
                }),
            ),
            (
                Some("q"),
                Some(&|app: &Application| {
                    assert!(!app.editor.file_tree_window.open);
                }),
            ),
        ],
        false,
    )
    .await
}

#[tokio::test(flavor = "multi_thread")]
async fn question_mark_opens_keymap_modal() -> anyhow::Result<()> {
    // `?` (Shift+/) shows the which-key style keymap modal while the tree is
    // focused: an `autoinfo` popup rendered by the editor exactly like the
    // one the `space` leader key shows, so it appears in the same position.
    // Both representations are accepted: the shifted character `?` (legacy
    // terminals) and the physical key `/` with the SHIFT modifier (Kitty
    // keyboard protocol / enhanced reporting). Any key closes it and runs
    // that binding: `?<esc>` dismisses without closing the tree, `?q`
    // dismisses and then closes the tree.
    let mut config = test_config();
    config.editor.file_tree.enable = true;
    let mut app = AppBuilder::new().with_config(config).build()?;
    test_key_sequences(
        &mut app,
        vec![
            (Some("<space>e"), None),
            (
                Some("?"),
                Some(&|app: &Application| {
                    assert!(app.editor.autoinfo.is_some());
                    assert!(app.editor.file_tree_window.open);
                }),
            ),
            (
                Some("<esc>"),
                Some(&|app: &Application| {
                    assert!(app.editor.autoinfo.is_none());
                    assert!(app.editor.file_tree_window.open);
                }),
            ),
            (
                Some("<S-/>"),
                Some(&|app: &Application| {
                    assert!(app.editor.autoinfo.is_some());
                }),
            ),
            (
                Some("<esc>"),
                Some(&|app: &Application| {
                    assert!(app.editor.autoinfo.is_none());
                }),
            ),
            (
                // Shift+? reported as the shifted character with the SHIFT
                // modifier (disambiguate-only terminals).
                Some("<S-?>"),
                Some(&|app: &Application| {
                    assert!(app.editor.autoinfo.is_some());
                }),
            ),
            (
                Some("?q"),
                Some(&|app: &Application| {
                    assert!(app.editor.autoinfo.is_none());
                    assert!(!app.editor.file_tree_window.open);
                }),
            ),
        ],
        false,
    )
    .await
}

#[tokio::test(flavor = "multi_thread")]
async fn window_commands_move_focus_between_tree_and_editor() -> anyhow::Result<()> {
    // The tree behaves like an ordinary helix window: `C-w w` rotates it into
    // the window cycle, `C-w h` jumps to it as the leftmost window, and
    // `C-w l` / `C-w w` from the tree hand focus back to the editor.
    let mut config = test_config();
    config.editor.file_tree.enable = true;
    let mut app = AppBuilder::new().with_config(config).build()?;
    test_key_sequences(
        &mut app,
        vec![
            (Some("<space>e"), None),
            (
                Some("<esc>"),
                Some(&|app: &Application| {
                    assert!(app.editor.file_tree_window.open);
                    assert!(!app.editor.file_tree_window.focused);
                }),
            ),
            // The tree is the next window in the rotation.
            (
                Some("<C-w>w"),
                Some(&|app: &Application| {
                    assert!(app.editor.file_tree_window.focused);
                }),
            ),
            // From the tree, `C-w w` moves to the next window (the editor).
            (
                Some("<C-w>w"),
                Some(&|app: &Application| {
                    assert!(!app.editor.file_tree_window.focused);
                }),
            ),
            // The tree is the leftmost window.
            (
                Some("<C-w>h"),
                Some(&|app: &Application| {
                    assert!(app.editor.file_tree_window.focused);
                }),
            ),
            // `C-w l` from the tree returns to the editor.
            (
                Some("<C-w>l"),
                Some(&|app: &Application| {
                    assert!(!app.editor.file_tree_window.focused);
                }),
            ),
        ],
        false,
    )
    .await
}

#[tokio::test(flavor = "multi_thread")]
async fn tree_deletes_selected_file() -> anyhow::Result<()> {
    // `d` / `Delete` deletes the selected entry, but only after a second
    // press confirms it.
    let dir = tempdir()?;
    std::fs::write(dir.path().join("a.txt"), "a")?;
    std::fs::write(dir.path().join("b.txt"), "b")?;
    let mut config = test_config();
    config.editor.file_tree.enable = true;
    let mut app = AppBuilder::new()
        .with_file(dir.path().to_path_buf(), Some(Default::default()))
        .with_config(config)
        .build()?;
    test_key_sequences(
        &mut app,
        vec![
            (Some("<space>e"), None),
            (Some("j"), None),
            (
                // One press only arms the deletion: nothing is deleted yet.
                Some("d"),
                Some(&|_: &Application| {
                    assert!(dir.path().join("a.txt").exists());
                }),
            ),
            (
                Some("d"),
                Some(&|_: &Application| {
                    assert!(!dir.path().join("a.txt").exists());
                    assert!(dir.path().join("b.txt").exists());
                }),
            ),
            (
                Some("dd"),
                Some(&|app: &Application| {
                    assert!(!dir.path().join("b.txt").exists());
                    assert!(app.editor.file_tree_window.open);
                }),
            ),
        ],
        false,
    )
    .await
}

#[tokio::test(flavor = "multi_thread")]
async fn tree_delete_is_cancelled_by_moving_selection() -> anyhow::Result<()> {
    // Arming the deletion and then moving the selection cancels it: the
    // follow-up `d` arms the new entry instead of deleting.
    let dir = tempdir()?;
    std::fs::write(dir.path().join("a.txt"), "a")?;
    std::fs::write(dir.path().join("b.txt"), "b")?;
    let mut config = test_config();
    config.editor.file_tree.enable = true;
    let mut app = AppBuilder::new()
        .with_file(dir.path().to_path_buf(), Some(Default::default()))
        .with_config(config)
        .build()?;
    test_key_sequences(
        &mut app,
        vec![
            (Some("<space>e"), None),
            (Some("jdj"), None),
            (
                Some("d"),
                Some(&|_: &Application| {
                    assert!(dir.path().join("a.txt").exists());
                    assert!(dir.path().join("b.txt").exists());
                }),
            ),
            (
                Some("d"),
                Some(&|_: &Application| {
                    // The second `d` confirmed the deletion of b.txt.
                    assert!(dir.path().join("a.txt").exists());
                    assert!(!dir.path().join("b.txt").exists());
                }),
            ),
        ],
        false,
    )
    .await
}

#[tokio::test(flavor = "multi_thread")]
async fn tree_deletes_selected_directory_recursively() -> anyhow::Result<()> {
    // `dd` on a directory removes it and everything below it.
    let dir = tempdir()?;
    std::fs::create_dir(dir.path().join("sub"))?;
    std::fs::write(dir.path().join("sub/inner.txt"), "i")?;
    let mut config = test_config();
    config.editor.file_tree.enable = true;
    let mut app = AppBuilder::new()
        .with_file(dir.path().to_path_buf(), Some(Default::default()))
        .with_config(config)
        .build()?;
    test_key_sequences(
        &mut app,
        vec![
            (Some("<space>e"), None),
            (Some("j"), None),
            (
                Some("dd"),
                Some(&|_: &Application| assert!(!dir.path().join("sub").exists())),
            ),
        ],
        false,
    )
    .await
}

#[tokio::test(flavor = "multi_thread")]
async fn tree_deletes_nonadjacent_marked_files() -> anyhow::Result<()> {
    let dir = tempdir()?;
    std::fs::write(dir.path().join("a.txt"), "a")?;
    std::fs::write(dir.path().join("b.txt"), "b")?;
    std::fs::write(dir.path().join("c.txt"), "c")?;
    let mut config = test_config();
    config.editor.file_tree.enable = true;
    let mut app = AppBuilder::new()
        .with_file(dir.path().to_path_buf(), Some(Default::default()))
        .with_config(config)
        .build()?;
    test_key_sequences(
        &mut app,
        vec![
            (Some("<space>e"), None),
            // Mark a.txt and c.txt, leaving the cursor on unmarked b.txt.
            (Some("j<space>jj<space>k"), None),
            (
                Some("d"),
                Some(&|_: &Application| {
                    assert!(dir.path().join("a.txt").exists());
                    assert!(dir.path().join("b.txt").exists());
                    assert!(dir.path().join("c.txt").exists());
                }),
            ),
            (
                Some("<del>"),
                Some(&|_: &Application| {
                    assert!(!dir.path().join("a.txt").exists());
                    assert!(dir.path().join("b.txt").exists());
                    assert!(!dir.path().join("c.txt").exists());
                }),
            ),
        ],
        false,
    )
    .await
}

#[tokio::test(flavor = "multi_thread")]
async fn tree_cancelled_batch_clear_marks_deletes_cursor_only() -> anyhow::Result<()> {
    let dir = tempdir()?;
    std::fs::write(dir.path().join("a.txt"), "a")?;
    std::fs::write(dir.path().join("b.txt"), "b")?;
    std::fs::write(dir.path().join("c.txt"), "c")?;
    let mut config = test_config();
    config.editor.file_tree.enable = true;
    let mut app = AppBuilder::new()
        .with_file(dir.path().to_path_buf(), Some(Default::default()))
        .with_config(config)
        .build()?;
    test_key_sequences(
        &mut app,
        vec![
            (Some("<space>e"), None),
            (Some("j<space>jj<space>"), None),
            (
                // Moving to b.txt cancels the batch confirmation.
                Some("dkd"),
                Some(&|_: &Application| {
                    assert!(dir.path().join("a.txt").exists());
                    assert!(dir.path().join("b.txt").exists());
                    assert!(dir.path().join("c.txt").exists());
                }),
            ),
            (
                // Cancel again, then clear the marks before deleting b.txt.
                Some("<esc><A-space><del><del>"),
                Some(&|_: &Application| {
                    assert!(dir.path().join("a.txt").exists());
                    assert!(!dir.path().join("b.txt").exists());
                    assert!(dir.path().join("c.txt").exists());
                }),
            ),
        ],
        false,
    )
    .await
}

#[tokio::test(flavor = "multi_thread")]
async fn tree_deletes_marked_descendant_after_collapsing_parent() -> anyhow::Result<()> {
    let dir = tempdir()?;
    std::fs::create_dir(dir.path().join("sub"))?;
    std::fs::write(dir.path().join("sub/a.txt"), "a")?;
    std::fs::write(dir.path().join("sub/b.txt"), "b")?;
    let mut config = test_config();
    config.editor.file_tree.enable = true;
    let mut app = AppBuilder::new()
        .with_file(dir.path().to_path_buf(), Some(Default::default()))
        .with_config(config)
        .build()?;
    test_key_sequences(
        &mut app,
        vec![
            (Some("<space>e"), None),
            // Expand sub, mark a.txt, then collapse sub with the cursor on it.
            (Some("jlj<space>hh"), None),
            (
                Some("dd"),
                Some(&|_: &Application| {
                    assert!(dir.path().join("sub").is_dir());
                    assert!(!dir.path().join("sub/a.txt").exists());
                    assert!(dir.path().join("sub/b.txt").exists());
                }),
            ),
        ],
        false,
    )
    .await
}

#[tokio::test(flavor = "multi_thread")]
async fn tree_renames_selected_file() -> anyhow::Result<()> {
    // `r` opens an inline rename bar prefilled with the name; typing a new
    // name and pressing `Enter` renames the entry on disk.
    let dir = tempdir()?;
    std::fs::write(dir.path().join("a.txt"), "content")?;
    let mut config = test_config();
    config.editor.file_tree.enable = true;
    let mut app = AppBuilder::new()
        .with_file(dir.path().to_path_buf(), Some(Default::default()))
        .with_config(config)
        .build()?;
    test_key_sequences(
        &mut app,
        vec![
            (Some("<space>e"), None),
            (Some("j"), None),
            // Clear the prefilled "a.txt", type the new name, confirm.
            (
                Some("r<backspace><backspace><backspace><backspace><backspace>b.txt<ret>"),
                Some(&|_: &Application| {
                    assert!(!dir.path().join("a.txt").exists());
                    let renamed = dir.path().join("b.txt");
                    assert!(renamed.exists());
                    assert_eq!(std::fs::read_to_string(renamed).unwrap(), "content");
                }),
            ),
        ],
        false,
    )
    .await
}

#[tokio::test(flavor = "multi_thread")]
async fn tree_renames_selected_directory() -> anyhow::Result<()> {
    // Renaming a directory keeps its contents and rewrites the tree paths.
    let dir = tempdir()?;
    std::fs::create_dir(dir.path().join("sub"))?;
    std::fs::write(dir.path().join("sub/inner.txt"), "i")?;
    let mut config = test_config();
    config.editor.file_tree.enable = true;
    let mut app = AppBuilder::new()
        .with_file(dir.path().to_path_buf(), Some(Default::default()))
        .with_config(config)
        .build()?;
    test_key_sequences(
        &mut app,
        vec![
            (Some("<space>e"), None),
            (Some("j"), None),
            (
                Some("r<backspace><backspace><backspace>newsub<ret>"),
                Some(&|_: &Application| {
                    assert!(!dir.path().join("sub").exists());
                    let renamed = dir.path().join("newsub");
                    assert!(renamed.is_dir());
                    assert_eq!(
                        std::fs::read_to_string(renamed.join("inner.txt")).unwrap(),
                        "i"
                    );
                }),
            ),
        ],
        false,
    )
    .await
}

#[tokio::test(flavor = "multi_thread")]
async fn tree_moves_marked_file_into_directory() -> anyhow::Result<()> {
    // `m` opens a destination bar prefilled with the selected entry's parent;
    // appending the destination and pressing `Enter` moves the marked entry
    // (a.txt) into it, keeping unmarked files in place.
    let dir = tempdir()?;
    std::fs::write(dir.path().join("a.txt"), "a")?;
    std::fs::write(dir.path().join("b.txt"), "b")?;
    std::fs::create_dir(dir.path().join("sub"))?;
    let mut config = test_config();
    config.editor.file_tree.enable = true;
    let mut app = AppBuilder::new()
        .with_file(dir.path().to_path_buf(), Some(Default::default()))
        .with_config(config)
        .build()?;
    test_key_sequences(
        &mut app,
        vec![
            (Some("<space>e"), None),
            // Mark a.txt (after the sub directory, which sorts first), leave
            // b.txt unmarked, then move into the sub directory (the bar is
            // prefilled with a.txt's parent, so append `/sub`).
            (Some("jj "), None),
            (
                Some("m/sub<ret>"),
                Some(&|_: &Application| {
                    assert!(!dir.path().join("a.txt").exists());
                    assert!(dir.path().join("sub/a.txt").exists());
                    assert!(dir.path().join("b.txt").exists());
                }),
            ),
        ],
        false,
    )
    .await
}

#[tokio::test(flavor = "multi_thread")]
async fn tree_picks_up_externally_created_file() -> anyhow::Result<()> {
    use helix_view::doc;

    // A file an external process creates under the tree root shows up
    // without a manual refresh: entering the filter re-reads the expanded
    // directories, after which the new file can be opened.
    let dir = tempdir()?;
    std::fs::write(dir.path().join("a.txt"), "a")?;
    let mut config = test_config();
    config.editor.file_tree.enable = true;
    let mut app = AppBuilder::new()
        .with_file(dir.path().to_path_buf(), Some(Default::default()))
        .with_config(config)
        .build()?;
    // Simulate an external process creating a file after the tree loaded.
    std::fs::write(dir.path().join("b.txt"), "b")?;
    test_key_sequences(
        &mut app,
        vec![
            (Some("j<ret>"), None), // open a.txt (sanity), focus moves to editor
            (Some("<C-w>h"), None), // focus the tree again
            // Entering the filter re-reads the root; the filter finds the
            // externally created b.txt and Enter opens it.
            (
                Some("/b<ret>"),
                Some(&|app: &Application| {
                    assert_eq!(doc!(app.editor).path().unwrap(), dir.path().join("b.txt"));
                }),
            ),
        ],
        false,
    )
    .await
}

#[tokio::test(flavor = "multi_thread")]
async fn align_selections_with_varying_columns() -> anyhow::Result<()> {
    test((
        indoc! {r"
            #[|]#I    I  II I
            IIIIIIIII
            IIIII
            IIIIIIIII
        "},
        r"%sI<ret>&gg",
        indoc! {r"
            #[I|]#    I  II I
            I    I  II IIIII
            I    I  II I
            I    I  II IIIII
        "},
    ))
    .await?;

    Ok(())
}
