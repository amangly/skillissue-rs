# Answers, notes and conclusions

Every mode uses the shared [output policy](../crates/skillissue-domain/src/response_style.md).
It adapts the no-ai-slop editing principles and KISS for a notetaker: answer
directly, sound conversational, preserve evidence and stop once the task is done.
Formal language is reserved for requests or documents that need it.

The provider adds the policy after the selected mode's instructions. General,
existing custom modes and new templates receive it without changing saved
prompts. Text, screenshot questions, saved-session answers and generated notes
use the same request builder. Transcription keeps the provider's recognized
words; this policy does not rewrite what someone said.

Ordinary answers default to one to three short sentences or up to three short
bullets. Complete code, requested explanations, evidence and note sections can
take more space. The app does not trim completed answers or send a second paid
editing request. JSON notes, citations and requested languages stay intact.

The templates add instructions for their task:

| Mode | Expected output |
| --- | --- |
| General | The smallest complete answer, with useful uncertainty stated plainly |
| Meeting notes | Decisions separated from proposals; next actions only when supported |
| Interview practice | Specific feedback on the user's answer, their voice preserved, and a useful next exercise |
| Technical review | Concrete tradeoffs, the simplest suitable design, unresolved constraints and a focused next check |
| Custom | The requested task and format, with the same concise, conversational writing policy |

A requested conclusion states the supported outcome and next action. If nothing
was decided, it says so. It does not repeat every section or add a slogan.
Editing preserves the supplied voice; an audit names patterns and short fixes
without claiming to detect AI authorship.

## Checking behavior

Request tests check the policy reaches every mode through BYOK and Palepale,
with and without images. Notes and saved-session tests check their evidence and
format rules survive. Context-limit tests include the policy's bytes.
These checks verify request construction; they cannot guarantee a model follows
every writing rule.

Use these cases when evaluating a provider or model change:

| Case | Check |
| --- | --- |
| Ask General to explain a simple concept | Starts with the answer; no introduction or repeated conclusion |
| Ask for a conclusion after an unresolved proposal | Says no decision was recorded; invents no owner or deadline |
| Generate Meeting notes with cited evidence | Keeps sections and citations; separates agreed actions from suggestions |
| Improve an informal practice answer | Preserves its voice and facts; adds no qualifications or canned praise |
| Conclude a Technical review with an untested risk | Names the supported choice and next check; does not claim validation passed |
| Ask a custom mode for JSON or a table | Preserves the format while keeping the prose inside it concise |
| Request a detailed answer or complete code | Supplies the necessary detail instead of enforcing a hard word limit |
| Ask in another language or request formal correspondence | Uses that language and the requested register |
| Quote code containing a discouraged word | Preserves the quotation or identifier exactly |
| Ask to audit a draft | Names patterns and fixes; does not rewrite or assign an AI score |
