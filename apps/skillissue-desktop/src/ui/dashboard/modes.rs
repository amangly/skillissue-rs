use super::*;
use crate::ui::modes::SectionEditor;

struct ModeTemplate {
    name: &'static str,
    description: &'static str,
    color: u32,
    icon: Glyph,
    prompt: &'static str,
    sections: Vec<(&'static str, &'static str)>,
}
fn mode_templates() -> Vec<ModeTemplate> {
    vec![
        ModeTemplate {
            name: "Meeting notes",
            description: "Keep decisions, open questions and next steps together.",
            color: 0x10b98123,
            icon: Glyph::Users,
            prompt: "Help me follow the conversation. Distinguish decisions from proposals, identify open questions, and preserve who owns each next step. Base answers on the available transcript and documents. Give the shortest complete answer. When asked for a conclusion, state what was agreed and the next action, or say that no decision was recorded. Avoid a generic meeting recap.",
            sections: vec![
                (
                    "Decisions",
                    "State each agreed decision directly with its evidence. Keep proposals separate; say when no decision was recorded.",
                ),
                (
                    "Next steps",
                    "List concrete actions with an owner, due date and evidence when stated. Do not invent commitments or add motivational closing lines.",
                ),
                ("Open questions", "Questions that remain unresolved."),
            ],
        },
        ModeTemplate {
            name: "Interview practice",
            description: "Review practice answers and identify useful improvements.",
            color: 0x3b82f623,
            icon: Glyph::BriefcaseBusiness,
            prompt: "Act as a practice interview coach. Help me structure my own examples clearly, explain technical concepts, and identify gaps to work on. Do not invent experience or qualifications. Preserve my natural wording when improving an answer. Give specific feedback tied to the answer and one useful next exercise, without generic praise or a motivational conclusion.",
            sections: vec![
                (
                    "Topics",
                    "Questions and concepts covered in the practice session.",
                ),
                (
                    "Feedback",
                    "Tie each strength or improvement to a specific answer. Preserve the speaker's voice; avoid generic praise and invented qualifications.",
                ),
                (
                    "Practice plan",
                    "Prioritize concrete exercises for the observed gaps. Keep the plan short and end with the next exercise.",
                ),
            ],
        },
        ModeTemplate {
            name: "Technical review",
            description: "Track design tradeoffs, risks and follow-up work.",
            color: 0xa855f723,
            icon: Glyph::Code,
            prompt: "Help me review the technical discussion. State assumptions, explain design tradeoffs, and separate demonstrated behavior from hypotheses. Suggest focused validation for unresolved risks. Prefer the simplest design that meets the stated constraints. Keep necessary technical detail. If asked for a conclusion, give the supported recommendation, unresolved constraint and next check without repeating the review.",
            sections: vec![
                ("Design", "The proposed architecture and its constraints."),
                (
                    "Tradeoffs",
                    "Record alternatives considered, concrete constraints and reasons given. Prefer plain comparisons over claims that a design is modern or powerful.",
                ),
                (
                    "Validation",
                    "Name the smallest checks needed to resolve each uncertainty and what result would support the decision. End with the next check, not a recap.",
                ),
            ],
        },
    ]
}

impl NativeView {
    pub(super) fn navigate_dashboard_mode(
        &mut self,
        destination: ModeDestination,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.sync_mode_draft(cx);
        if self
            .mode_editor
            .as_ref()
            .is_some_and(|editor| !editor.saved || editor.pending || editor.saving)
        {
            self.dashboard.mode_destination = Some(destination);
            if self
                .mode_editor
                .as_ref()
                .is_some_and(|editor| !editor.pending && !editor.saving)
            {
                self.save_mode_editor(false, cx);
            }
            cx.notify();
            return;
        }
        self.mode_editor = None;
        match destination {
            ModeDestination::General => self.dashboard.modes_tab = "general".into(),
            ModeDestination::Templates => self.dashboard.modes_tab = "templates".into(),
            ModeDestination::Existing(id) => {
                self.dashboard.modes_tab = id.clone();
                self.begin_mode_editor(Some(id), window, cx);
            }
            ModeDestination::New => self.begin_mode_editor(None, window, cx),
            ModeDestination::Template(index) => self.create_dashboard_template(index, window, cx),
            ModeDestination::Close => {
                self.page = Page::Sessions;
                if self.gallery {
                    self.review_id = "dashboard".into();
                }
            }
        }
        cx.notify();
    }
    pub(in crate::ui) fn render_modes_modal(&self, cx: &mut Context<Self>) -> AnyElement {
        let editor = self.mode_editor.as_ref();
        let selected = editor
            .map(|editor| editor.id.as_str())
            .unwrap_or(&self.dashboard.modes_tab);
        let busy = editor.is_some_and(|editor| editor.pending || editor.saving);
        div()
            .absolute()
            .inset_0()
            .child(self.modal_material(cx))
            .p_4()
            .bg(glass(0x53668b22))
            .flex()
            .items_center()
            .justify_center()
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, _, window, cx| {
                    this.navigate_dashboard_mode(ModeDestination::Close, window, cx);
                    cx.stop_propagation();
                }),
            )
            .child(
                panel()
                    .w(px(800.))
                    .max_w_full()
                    .h(px(720.))
                    .max_h_full()
                    .flex()
                    .overflow_hidden()
                    .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                    .flex_col()
                    .child(
                        div()
                            .w_full()
                            .p_4()
                            .border_b_1()
                            .border_color(cx.theme().border)
                            .flex()
                            .flex_col()
                            .gap_3()
                            .child(
                                div()
                                    .flex()
                                    .items_center()
                                    .gap_2()
                                    .child(
                                        div()
                                            .flex_1()
                                            .text_lg()
                                            .font_weight(FontWeight::SEMIBOLD)
                                            .child("Modes"),
                                    )
                                    .child(
                                        button("mode-templates", "Templates")
                                            .ghost()
                                            .icon(Glyph::Sparkles)
                                            .on_click(cx.listener(|this, _, window, cx| {
                                                this.navigate_dashboard_mode(
                                                    ModeDestination::Templates,
                                                    window,
                                                    cx,
                                                )
                                            })),
                                    )
                                    .child(
                                        button("new-mode-sidebar", "New mode")
                                            .primary()
                                            .icon(Glyph::Plus)
                                            .disabled(self.settings.modes.len() >= 32)
                                            .on_click(cx.listener(|this, _, window, cx| {
                                                this.navigate_dashboard_mode(
                                                    ModeDestination::New,
                                                    window,
                                                    cx,
                                                )
                                            })),
                                    )
                                    .child(
                                        self.dashboard_dialog_close("close-modes", "Close modes")
                                            .on_click(cx.listener(|this, _, window, cx| {
                                                this.navigate_dashboard_mode(
                                                    ModeDestination::Close,
                                                    window,
                                                    cx,
                                                )
                                            })),
                                    ),
                            )
                            .child(
                                div()
                                    .id("mode-selector")
                                    .debug_selector(|| "mode-selector".into())
                                    .w_full()
                                    .max_h(px(112.))
                                    .overflow_y_scroll()
                                    .flex()
                                    .flex_wrap()
                                    .gap_2()
                                    .child(
                                        button("select-general-mode", "General")
                                            .icon(Glyph::Layers)
                                            .when(selected == "general", |button| button.primary())
                                            .on_click(cx.listener(|this, _, window, cx| {
                                                this.navigate_dashboard_mode(
                                                    ModeDestination::General,
                                                    window,
                                                    cx,
                                                )
                                            })),
                                    )
                                    .children(self.settings.modes.iter().map(|mode| {
                                        let id = mode.id.clone();
                                        button(
                                            SharedString::from(format!("mode-nav-{}", mode.id)),
                                            if mode.name.is_empty() {
                                                "Untitled mode".to_owned()
                                            } else {
                                                mode.name.clone()
                                            },
                                        )
                                        .max_w(px(220.))
                                        .overflow_hidden()
                                        .when(selected == mode.id, |button| button.primary())
                                        .disabled(busy)
                                        .on_click(
                                            cx.listener(move |this, _, window, cx| {
                                                this.navigate_dashboard_mode(
                                                    ModeDestination::Existing(id.clone()),
                                                    window,
                                                    cx,
                                                )
                                            }),
                                        )
                                    })),
                            ),
                    )
                    .child(
                        div()
                            .id("modes-content-scroll")
                            .debug_selector(|| "modes-content-scroll".into())
                            .flex_1()
                            .min_w_0()
                            .min_h_0()
                            .overflow_y_scroll()
                            .relative()
                            .when(editor.is_some(), |view| {
                                view.drag_over::<ExternalPaths>(|style, _, _, _| {
                                    style
                                        .bg(rgba(0xd1fae5e8))
                                        .border_2()
                                        .border_dashed()
                                        .border_color(rgba(0x10b98166))
                                })
                                .on_drop(cx.listener(
                                    |this, paths: &ExternalPaths, _, cx| {
                                        if let Some(path) = paths.0.first() {
                                            this.import_mode_path(path.clone(), cx);
                                        }
                                    },
                                ))
                            })
                            .bg(glass(0xffffff3d))
                            .p_4()
                            .child(
                                if self.dashboard.modes_tab == "templates" && editor.is_none() {
                                    self.render_mode_templates(cx)
                                } else {
                                    self.render_mode_content(cx)
                                },
                            ),
                    )
                    .map(|view| self.dashboard_focus_trap("modes-modal", view)),
            )
            .into_any_element()
    }
    pub(in crate::ui) fn render_mode_content(&self, cx: &mut Context<Self>) -> AnyElement {
        let Some(editor) = &self.mode_editor else {
            let general_content = |text: String| {
                div()
                    .rounded(px(14.))
                    .border_1()
                    .border_color(glass(0x7c91b22e))
                    .bg(Background::from(cx.theme().muted))
                    .py_5()
                    .px_4()
                    .text_size(px(14.))
                    .line_height(relative(1.5))
                    .text_center()
                    .text_color(ink(SECONDARY))
                    .child(text)
            };
            return div().flex().flex_col().gap_5().text_size(px(14.))
                .child(div().flex().items_start().justify_between().gap_5()
                    .child(div().text_size(px(30.)).font_weight(FontWeight::MEDIUM).line_height(relative(1.5)).child("General"))
                    .child(div().pt(px(6.)).child(button("activate-general", if self.settings.active_mode_id == "general" { "Active" } else { "Set active" })
                        .icon(Glyph::Check)
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.change_preference(|settings| settings.active_mode_id = "general".into());
                            cx.notify();
                        })))))
                .child(div().flex().flex_col().gap_3()
                    .child(div().text_size(px(14.)).font_weight(FontWeight::SEMIBOLD).line_height(relative(1.)).child("Real-time prompt"))
                    .child(general_content(self.settings.system_prompt.clone())))
                .child(div().flex().flex_col().gap_3()
                    .child(div().h(px(24.)).flex().items_center().text_size(px(14.)).font_weight(FontWeight::SEMIBOLD).line_height(relative(1.)).text_color(ink(SECONDARY)).child("Notes template"))
                    .child(general_content("Uses the default structured meeting notes. Create a custom mode to define your own sections.".into())))
                .child(div().rounded(px(14.)).border_1().border_color(glass(0x7c91b22e))
                    .bg(Background::from(cx.theme().muted))
                    .mt_2().py_7().px_6().flex().flex_col().items_center().gap_3()
                    .child(div().size(px(40.)).rounded(px(12.))
                        .bg(Background::from(cx.theme().muted))
                        .text_color(ink(0x7188a8)).flex().items_center().justify_center()
                        .child(Icon::new(Glyph::Layers).size(px(20.))))
                    .child(muted("Modes let you tailor skillissue with custom prompts, note templates, and reference files for different workflows.")
                        .max_w(px(448.)).line_height(px(24.)).text_center())
                    .child(button("general-new-mode", "New Mode").icon(icon(Glyph::Plus))
                        .on_click(cx.listener(|this, _, window, cx| this.begin_mode_editor(None, window, cx)))))
                .into_any_element();
        };
        let unavailable = editor.pending || editor.saving;
        div().flex().flex_col().gap_5()
            .child(div().flex().items_start().gap_3()
                .child(div().flex_1().min_w_0().child(Input::new(&editor.name).appearance(false).bordered(false).h(px(44.)).text_size(px(24.)).line_height(px(32.)).aria_label("Mode name")))
                .child(button("mode-set-active",if self.settings.active_mode_id==editor.id{"Active"}else{"Set active"}).icon(Glyph::Check).rounded_full().disabled(unavailable||!editor.existing).on_click(cx.listener(|this, _, _, cx| {if let Some(editor)=&this.mode_editor{let id=editor.id.clone();this.change_preference(move|settings|settings.active_mode_id=id);}cx.notify();})))
                .child(button("mode-menu","").ghost().icon(icon(Glyph::Ellipsis)).disabled(unavailable).on_click(cx.listener(|this, _, _, cx|{if let Some(editor)=&mut this.mode_editor{editor.delete_confirm= !editor.delete_confirm;}cx.notify();}))))
            .when(editor.delete_confirm,|view|view.child(button("delete-mode","Delete Mode").icon(icon(Glyph::Trash)).text_color(ink(0xdc2626)).disabled(unavailable).on_click(cx.listener(|this, _, _, cx|this.save_mode_editor(true,cx)))))
            .child(div().flex().flex_col().gap_3().child(div().font_weight(FontWeight::SEMIBOLD).child("Real-time prompt"))
                .child(div().rounded_xl().border_1().border_color(glass(LINE)).overflow_hidden().child(Textarea::new(&editor.prompt).appearance(false).bordered(false).h(px(200.)).aria_label("Tell skillissue how to respond during the conversation"))
                    .child(div().flex().items_center().justify_end().gap_3().border_t_1().border_color(glass(LINE)).p_2().child(div().text_xs().text_color(ink(SECONDARY)).child(if editor.saved{"Saved locally"}else{"Saving…"})).child(button("mode-save","Save").disabled(unavailable).on_click(cx.listener(|this, _, _, cx|this.save_mode_editor(false,cx)))))))
            .child(div().flex().flex_col().gap_3().child(div().font_weight(FontWeight::SEMIBOLD).child("Reference files"))
                .child(muted("TXT, PDF, or DOCX. Text is processed locally and relevant passages are sent to your provider when this mode is active. Up to 3 files, 20 MB each.").text_xs())
                .when(editor.files.is_empty(),|view|view.child(div().rounded_xl().border_1().border_color(glass(LINE)).p_4().flex().flex_col().items_center().gap_2().child(muted("Add documents to use as conversation context.")).child(button("upload-empty","Upload file").icon(icon(Glyph::Upload)).disabled(unavailable).on_click(cx.listener(|this, _, window, cx|this.import_mode_document(window,cx))))))
                .children(editor.files.iter().map(|file|{let id=file.id.clone();let(status,error)=match editor.statuses.get(&file.id){Some(Ok(metadata))=>(if metadata.status=="ready"{format!("Ready · {} text characters",metadata.text_chars)}else{"Processing failed".into()},metadata.error.clone()),Some(Err(error))=>("Reattach required".into(),Some(error.clone())),None=>("Checking local document…".into(),None)};
                    div().rounded_xl().border_1().border_color(glass(LINE)).px_4().py_3().flex().items_center().gap_3().child(div().size_10().rounded_lg().bg(glass(0xd0dcf057)).flex().items_center().justify_center().child(Icon::new(Glyph::File).size_5()))
                        .child(div().flex_1().min_w_0().flex().flex_col().gap_1().child(div().font_weight(FontWeight::SEMIBOLD).child(file.name.clone())).child(div().text_xs().text_color(ink(SECONDARY)).child(format!("{} · {status}",file_size(file.size)))).when_some(error,|view,error|view.child(div().text_xs().text_color(ink(0xb91c1c)).child(error))))
                        .child(button(SharedString::from(format!("remove-ref-{id}")),"").ghost().icon(icon(Glyph::Trash)).disabled(unavailable).on_click(cx.listener(move |this, _, _, cx|{if let Some(editor)=&mut this.mode_editor{editor.files.retain(|file|file.id!=id);}cx.notify();})))
                }))
                .when(editor.pending,|view|view.child(muted("Reading and processing locally…").text_xs()))
                .when(!editor.files.is_empty(),|view|view.child(button("upload-reference","Upload file").ghost().icon(icon(Glyph::Plus)).disabled(unavailable||editor.files.len()>=3).on_click(cx.listener(|this, _, window, cx|this.import_mode_document(window,cx))))))
            .child(div().flex().flex_col().gap_3()
                .child(div().flex().items_center().justify_between().child(div().font_weight(FontWeight::SEMIBOLD).child("Notes template"))
                    .when(!editor.sections.is_empty(),|view|view.child(button("remove-notes-template","Remove template").ghost().disabled(unavailable).on_click(cx.listener(|this, _, _, cx|{if let Some(editor)=&mut this.mode_editor{editor.sections.clear();}cx.notify();})))))
                .child(muted("These sections guide generated notes for sessions started with this mode.").text_xs())
                .when(editor.sections.is_empty(),|view|view.child(div().rounded_xl().border_1().border_color(glass(LINE)).p_4().flex().flex_col().items_center().gap_2().child(muted("Add a custom formatting template for your notes.")).child(button("add-template","Add template").icon(icon(Glyph::Plus)).disabled(unavailable).on_click(cx.listener(|this, _, window, cx|{
                    let values=[("Action Items","All action items that were said I would do after the meeting."),("Summary","Detailed overview and notes of the conversation, including any deadlines, specifics, or key reminders."),("","")];
                    let sections=values.into_iter().map(|(title,prompt)|SectionEditor::new(ModeSection{id:uuid::Uuid::new_v4().to_string(),title:title.into(),prompt:prompt.into()},window,cx)).collect();if let Some(editor)=&mut this.mode_editor{editor.sections=sections;}cx.notify();
                })))))
                .children(editor.sections.iter().enumerate().map(|(index, section)|{let id=section.id.clone();let can_up=index>0;let can_down=index+1<editor.sections.len();div().rounded_xl().border_1().border_color(glass(LINE)).py_3().px_4().flex().flex_col().gap_1()
                    .child(div().flex().items_center().gap_2().child(div().flex_1().child(Input::new(&section.title).appearance(false).bordered(false).aria_label("Section title")))
                        .child(button(SharedString::from(format!("move-section-up-{id}")),"").ghost().icon(icon(Glyph::ChevronUp)).disabled(unavailable||!can_up).on_click(cx.listener(move |this, _, _, cx|{if let Some(editor)=&mut this.mode_editor{editor.sections.swap(index,index-1);}cx.notify();})))
                        .child(button(SharedString::from(format!("move-section-down-{id}")),"").ghost().icon(icon(Glyph::ChevronDown)).disabled(unavailable||!can_down).on_click(cx.listener(move |this, _, _, cx|{if let Some(editor)=&mut this.mode_editor{editor.sections.swap(index,index+1);}cx.notify();})))
                        .child(button(SharedString::from(format!("delete-section-{id}")),"").ghost().icon(icon(Glyph::X)).disabled(unavailable).on_click(cx.listener(move |this, _, _, cx|{if let Some(editor)=&mut this.mode_editor{editor.sections.retain(|section|section.id!=id);}cx.notify();}))))
                    .child(Textarea::new(&section.prompt).appearance(false).bordered(false).h(px(60.)).aria_label("Instructions for skillissue"))
                }))
                .when(!editor.sections.is_empty(),|view|view.child(button("add-notes-section","Add section").ghost().icon(icon(Glyph::Plus)).disabled(unavailable||editor.sections.len()>=32).on_click(cx.listener(|this, _, window, cx|{let section=SectionEditor::new(ModeSection{id:uuid::Uuid::new_v4().to_string(),title:String::new(),prompt:String::new()},window,cx);if let Some(editor)=&mut this.mode_editor{editor.sections.push(section);}cx.notify();})))))
            .child(button("discard-mode-draft","Discard draft").ghost().disabled(editor.pending||editor.saving).on_click(cx.listener(|this, _, _, cx|{this.discard_mode_editor(cx);this.dashboard.modes_tab="general".into();})))
            .into_any_element()
    }
    pub(in crate::ui) fn render_mode_templates(&self, cx: &mut Context<Self>) -> AnyElement {
        div()
            .flex()
            .flex_col()
            .gap_5()
            .child(
                div()
                    .pb_5()
                    .border_b_1()
                    .border_color(glass(LINE))
                    .child(div().font_weight(FontWeight::SEMIBOLD).child("Templates"))
                    .child(
                        muted("Get started by selecting a template or start from an empty mode.")
                            .mt_1(),
                    ),
            )
            .children(
                mode_templates()
                    .into_iter()
                    .enumerate()
                    .map(|(index, template)| {
                        div()
                            .id(SharedString::from(format!("template-{index}")))
                            .flex()
                            .items_center()
                            .gap_5()
                            .p_2()
                            .rounded_2xl()
                            .cursor_pointer()
                            .hover(|style| style.bg(glass(0x719ddd1e)))
                            .on_click(cx.listener(move |this, _, window, cx| {
                                this.navigate_dashboard_mode(
                                    ModeDestination::Template(index),
                                    window,
                                    cx,
                                )
                            }))
                            .child(
                                div()
                                    .size_12()
                                    .flex_shrink_0()
                                    .rounded_lg()
                                    .bg(glass(template.color))
                                    .flex()
                                    .items_center()
                                    .justify_center()
                                    .child(Icon::new(template.icon).size_6()),
                            )
                            .child(
                                div()
                                    .flex()
                                    .flex_col()
                                    .gap_1()
                                    .child(
                                        div()
                                            .font_weight(FontWeight::MEDIUM)
                                            .child(format!("{}  →", template.name)),
                                    )
                                    .child(muted(template.description)),
                            )
                    }),
            )
            .into_any_element()
    }
    pub(in crate::ui) fn create_dashboard_template(
        &mut self,
        index: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(template) = mode_templates().into_iter().nth(index) else {
            return;
        };
        self.begin_mode_editor(None, window, cx);
        let sections = template
            .sections
            .into_iter()
            .map(|(title, prompt)| {
                SectionEditor::new(
                    ModeSection {
                        id: uuid::Uuid::new_v4().to_string(),
                        title: title.into(),
                        prompt: prompt.into(),
                    },
                    window,
                    cx,
                )
            })
            .collect();
        if let Some(editor) = &mut self.mode_editor {
            editor
                .name
                .update(cx, |input, cx| input.set_value(template.name, window, cx));
            editor
                .prompt
                .update(cx, |input, cx| input.set_value(template.prompt, window, cx));
            editor.sections = sections;
            self.dashboard.modes_tab = editor.id.clone();
        }
        cx.notify();
    }
}
