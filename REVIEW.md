# コードベースレビュー（2026-11-01）

対象: `00e364a079fb275a51f4ed30fc0dc852d26a23f6` / v0.2.3。

修正の詳細計画: [REMEDIATION_PLAN.md](REMEDIATION_PLAN.md)（優先順位、依存関係、作業単位、回帰テスト、完了条件）。

設計・アーキテクチャの評価と変更方針: [ARCHITECTURE_REMEDIATION.md](ARCHITECTURE_REMEDIATION.md)。

## 結論・検証範囲

PRのインラインコメントは現状のMCPでは取得・投稿できない。通常コメントのAPIを使用しており、行コメントとは保存先も情報構造も異なる。加えて、一覧の欠落、PR本文の消失、マイルストーンの意図しない変更など、業務利用に影響する問題がある。

`src/` 全体、テスト、README、運用文書、同梱Skill、CI・リリース・E2Eスクリプトを確認した。GitBucketはDocker E2E指定の4.44.0と、調査時点の最新リリース4.48.0の公式ソースを照合した。利用中の業務インスタンスのバージョン、プラグイン、認証方式は不明であり、業務環境へのアクセスや更新は行っていない。実GitBucketに対するE2Eは今回実行していない。

確認結果:

- 既存 `cargo test --locked`: 191件成功（unit 122 / API 48 / MCP 21）、実GitBucket用17件はignore。
- `cargo fmt --all --check`: 成功。
- `cargo clippy --all-targets --all-features --locked -- -D warnings`: 成功。
- 一時的な再現テスト8件で、25件での打切り、100件以上の非ページング一覧の反復、Webの検証エラーを成功扱い、Unicode日付でのpanic、既存設定の0644維持、インライン指定の黙殺、URLフラグメントによる対象変化、Debugのパスワード露出を確認。
- 追加のバイナリ確認で、`--help`が通常起動になりready前で入力待ちになることと、不正TOML内の架空トークンがstderrに現れることも確認。
- 再現テストは不具合を正しい仕様として固定しないためテストスイートから除外した。今回の実行環境では `/tmp/gb-review-probes.rs` に保存。将来の修正時には期待値を反転して正式な回帰テストにする。

重要度: **P1** = 優先修正（データ消失、重要機能の欠落・誤動作）、**P2** = 通常修正（条件付き誤動作、信頼性・安全性）、**P3** = 改善。以下では「再現済み」「公式実装との照合」「設計上のリスク」を区別する。未発見の問題がないことを保証するレビューではない。

## 優先修正事項

### F01 [P1] PRインラインコメントの取得・投稿が未実装

箇所: `src/api/pull_request.rs:86`、`src/tools/pull_request.rs:82`、`src/models/comment.rs:6`、`src/api/mod.rs`。

- 投稿先は `POST /repos/{owner}/{repo}/issues/{number}/comments`。PR全体の会話コメントを作成する。
- 入力はowner/repo/PR番号/bodyのみ。path、commit、旧行・新行、スレッドへの返信を指定できない。
- `Comment`型にもファイル・行・コミット情報がない。取得用PRレビューコメントツール自体がない。
- `list_issue_comments`にPR番号を渡せば通常コメントは取得できるが、インラインコメントは取得できない。`get_pull_request`もコメントを取得しない。
- GitBucketのIssueコメントAPIは `IssueComments` から通常コメント等を取得する。インラインは `CommitComment` で、WebのPR会話画面は両者を統合している。Issue APIを呼び直すだけでは解決しない。

**API側の制約:** 確認した4.44.0/4.48.0のPR RESTコントローラにGitHub互換の `GET/POST /pulls/{id}/comments` はない。レスポンスに `review_comments_url` が存在しても、対応ルートが実装されているとは限らない。`ApiPullRequestReviewComment`はWebhook等の出力型であり、CRUD APIの存在証明ではない。

**修正方針:** 通常コメントとインラインを別ツール・別モデルにする。安定した方法はGitBucketプラグイン等で必要なAPIを補完すること。Webフォールバックを使うなら、PRページ/ファイル画面とコミットコメントのフォームを専用アダプタで扱う。公式Web側には `POST /{owner}/{repo}/commit/{sha}/comment/new` と `_data/new` があり、`fileName`, `oldLineNumber`, `newLineNumber`, `content`, `issueId`, `diff`を受け取る。GitHubの `position` をそのまま移植しない。現在/元のコミットと行位置、ファイル全体コメント、削除行、コミット更新後の位置移動も考慮する。HTML経由の取得でMarkdown原文を完全に回復できるかは別途検証が必要。

根拠: [IssueコメントAPI](https://github.com/gitbucket/gitbucket/blob/4.48.0/src/main/scala/gitbucket/core/controller/api/ApiIssueCommentControllerBase.scala)、[PR REST API](https://github.com/gitbucket/gitbucket/blob/4.48.0/src/main/scala/gitbucket/core/controller/api/ApiPullRequestControllerBase.scala)、[Webコミットコメント](https://github.com/gitbucket/gitbucket/blob/4.44.0/src/main/scala/gitbucket/core/controller/RepositoryViewerController.scala#L834)、[PR会話統合](https://github.com/gitbucket/gitbucket/blob/4.44.0/src/main/scala/gitbucket/core/service/PullRequestService.scala#L541)。

### F02 [P1] Issue/PR一覧が25件で打ち切られる

箇所: `src/api/client.rs:204`、`src/api/issue.rs:11`、`src/api/pull_request.rs:13`。

`per_page=100`を要求し、応答が100件未満なら最終ページと判断する。しかしGitBucketのIssue/PR APIは `PullRequestLimit = 25` で検索しており、要求のper_pageを使用しない。26件以上存在しても最初の25件だけを成功として返す。closed/all指定でも同じ。**25件の1ページ目と26件目の2ページ目を用意したモックで、2ページ目が呼ばれないことを再現した。**

**修正方針:** エンドポイントごとのページ方式を実装する。Issue/PRは25件固定の契約を扱い、空ページ等で終端を判定する。単に全APIで「空になるまで」に変えるとF03を悪化させる。

根拠: [Issue一覧](https://github.com/gitbucket/gitbucket/blob/4.48.0/src/main/scala/gitbucket/core/controller/api/ApiIssueControllerBase.scala#L26)、[PR一覧](https://github.com/gitbucket/gitbucket/blob/4.48.0/src/main/scala/gitbucket/core/controller/api/ApiPullRequestControllerBase.scala#L33)、[固定上限](https://github.com/gitbucket/gitbucket/blob/4.44.0/src/main/scala/gitbucket/core/service/PullRequestService.scala#L641)。

### F03 [P1] ページ分割しない一覧は100件以上で必ず反復・失敗する

箇所: `src/api/client.rs:204`、repository/branch/label/milestone/commentの各list処理。

確認した公式実装ではリポジトリ一覧、ブランチ、ラベル、マイルストーン、Issueコメントは、それぞれ全件等を返しpage/per_pageを使用しない。100件以上返ると同じ応答を100回取得し、最後に `Pagination exceeded 100 pages` で全体が失敗する。99件では動くのに100件になると壊れる。**ブランチ100件の固定応答で100リクエスト後の失敗を再現した。**

**修正方針:** 非ページングAPIは1回だけ取得。バージョン・拡張APIに対応する場合も方式を明示し、同一ページ検知と結果件数/サイズ制御を入れる。

根拠: [Repository](https://github.com/gitbucket/gitbucket/blob/4.44.0/src/main/scala/gitbucket/core/controller/api/ApiRepositoryControllerBase.scala)、[Branch](https://github.com/gitbucket/gitbucket/blob/4.44.0/src/main/scala/gitbucket/core/controller/api/ApiRepositoryBranchControllerBase.scala)、[Label](https://github.com/gitbucket/gitbucket/blob/4.48.0/src/main/scala/gitbucket/core/controller/api/ApiIssueLabelControllerBase.scala)、[Milestone](https://github.com/gitbucket/gitbucket/blob/4.48.0/src/main/scala/gitbucket/core/controller/api/ApiIssueMilestoneControllerBase.scala)、[Issue comment](https://github.com/gitbucket/gitbucket/blob/4.48.0/src/main/scala/gitbucket/core/controller/api/ApiIssueCommentControllerBase.scala)。

### F04 [P1] PRタイトルだけの変更で既存本文が消える／本文だけの変更は無視される

箇所: `src/api/pull_request.rs:50`、`src/tools/pull_request.rs:176`、`src/models/pull_request.rs:43`。

GitBucketの `updatePullRequestsByApi` は `title.foreach` の中でだけ `updateIssue(..., title, body)` を呼ぶ。body省略は本文維持ではなくNoneになる。したがって:

| MCP要求 | 公式実装から確認できる結果 |
| --- | --- |
| `title`のみ | 本文をNoneへ更新し、既存本文を消す |
| `body`のみ | 本文更新のコードを通らず、変更されない |
| 存在しない`base` | 対象ブランチを発見できなくても正常応答になり得る |

4.44.0と4.48.0の公式ソースで確認。MCPは正常HTTP応答をそのまま成功にしており、404フォールバックでは救済できない。業務インスタンスでの再現は未実施。

**修正方針:** 部分更新をGitBucket用に変換し、タイトル/本文の片方だけなら現在値を補完する。要求したフィールドを返却値または再取得結果と比較して未反映を検出する。無効なbaseも事前/事後に検証する。

根拠: [4.48.0更新実装](https://github.com/gitbucket/gitbucket/blob/4.48.0/src/main/scala/gitbucket/core/service/PullRequestService.scala#L309)、[4.44.0更新実装](https://github.com/gitbucket/gitbucket/blob/4.44.0/src/main/scala/gitbucket/core/service/PullRequestService.scala#L297)、[本文置換処理](https://github.com/gitbucket/gitbucket/blob/4.44.0/src/main/scala/gitbucket/core/service/IssuesService.scala#L601)。

### F05 [P1] マイルストーンの部分更新が置換APIと不整合

箇所: `src/api/milestone.rs:50`、`src/models/milestone.rs:42`。

MCPは変更フィールドだけPATCHするが、GitBucketは作成と同じ `CreateAMilestone`（title必須、state既定open）を使い、description/due_onをそのまま置換する。

- 閉じたマイルストーンにtitleだけ送ると、stateがopenになり、既存descriptionとdue_onも消え得る。
- stateだけ/descriptionだけ等ではtitle不足になり、JSON抽出失敗の404からWebフォールバックへ進む。REST更新APIが存在するのにWebパスワードが必要になる。
- 本来不要なパスワードを設定しても、title付きの危険な成功経路には対処できない。

**修正方針:** 現在値を取得し、title/state/description/due_onを補完したGitBucket専用リクエストを使う。省略と削除を区別する。titleだけ変更した際の他フィールド保持を回帰テストにする。

根拠: [更新API](https://github.com/gitbucket/gitbucket/blob/4.48.0/src/main/scala/gitbucket/core/controller/api/ApiIssueMilestoneControllerBase.scala#L85)、[入力型](https://github.com/gitbucket/gitbucket/blob/4.44.0/src/main/scala/gitbucket/core/api/CreateAMilestone.scala)。

### F06 [P2] ラベルの色だけを変更すると422になり、Web救済も働かない

箇所: `src/api/label.rs:43`、`src/api/label.rs:73`。

色だけ変更する場合、現在のラベル名をnameとして補完する。GitBucketのPATCHはそのnameが既存かを調べ、自分自身も重複扱いして422を返す。フォールバックは404のみなので色変更が失敗する。READMEは「REST更新が未対応の時だけWebへ」と説明するが、これはルート未実装ではなく、実装済みルートの制約である。既存E2Eは名前と色を同時変更するため見逃す。

**修正方針:** 同名更新の既知制約を能力判定に含め、意図を確認できたケースだけWebへ変換する。422全般を無条件にフォールバックさせると、本物の重複名等を隠すため避ける。

根拠: [GitBucketラベルPATCH](https://github.com/gitbucket/gitbucket/blob/4.48.0/src/main/scala/gitbucket/core/controller/api/ApiIssueLabelControllerBase.scala#L72)。

### F07 [P2] ラベルdescriptionを指定できるが標準GitBucketでは保存できない

箇所: `src/models/label.rs:15`、`src/api/label.rs:66`、`src/api/label.rs:134`、`src/tools/label.rs:42`。

標準GitBucketの `CreateALabel` はname/colorのみ。作成時や名前変更と同時にdescriptionを送ると無視され得る。Webフォールバックでも名前/色を変える場合はdescriptionを黙って捨てる。一方descriptionだけなら別のエラーになり、一貫しない。READMEに一部説明があっても、ツールの入力説明と成功結果では反映されなかったことが伝わらない。

**修正方針:** サポートしない環境では変更前に拒否するか、`applied_fields` / `unsupported_fields`を返す。名前や色の変更後に初めてエラーとしない。

根拠: [標準入力型](https://github.com/gitbucket/gitbucket/blob/4.44.0/src/main/scala/gitbucket/core/api/CreateALabel.scala)。

### F08 [P2] WebフォールバックがHTMLの検証エラーを成功扱いする

箇所: `src/api/web.rs:245`、`src/api/milestone.rs:138`、`src/api/milestone.rs:252`。

HTTP 2xx/3xxと最終URLがsignin以外であることしか確認しない。Webフォームの入力エラー画面は200になり得る。**「Milestone already exists」という200 HTMLを返してもcreate_milestoneがOkになることを再現。**作成後は同名の最大番号を探すため、作成が失敗しても既存マイルストーンを「作成した結果」として返す可能性もある。削除では削除後の存在確認がない。Issue/Label/Milestoneの更新後GETも要求値との一致を検証しない。

**修正方針:** 成功リダイレクト、フォームエラー、認証済み画面を区別し、作成IDと変更後の値を確認する。成功と判定できない場合は結果不明として返す。

### F09 [P2] 404をAPI未実装と断定し、誤ったフォールバック・診断になる

箇所: `src/api/issue.rs:99`、`src/api/label.rs:94`、`src/api/milestone.rs:92`、`src/api/pull_request.rs:102`。

対象がGETできれば「更新API非対応」とみなす。しかしGitBucketはルート未実装以外にも、入力抽出・形式チェックの失敗で404を返す。F05のtitle不足、マイルストーン名のREST検証などが該当。権限を404で隠す構成も区別できない。「パスワードを設定してください」が真因から外れ、RESTとWebで異なる権限の処理を試すことになる。

**修正方針:** バージョン・能力とエンドポイント別の契約を管理する。非対応/入力不正/存在しない/権限不明を分離し、元のエラーとフォールバック経路を保持する。

### F10 [P2] 複数更新の途中失敗と、更新済み・応答失敗を区別できない

箇所: `src/api/issue.rs:170`、`src/api/milestone.rs:227`、`src/api/pull_request.rs:134`、`src/tools/response.rs:48`。

Webではタイトル→本文→状態と個別に変更する。途中で失敗すると前段は保存済みでも、結果は通常のエラーだけ。書込み成功後のGET/JSON解釈失敗も同様。新規作成やコメントPOSTが保存済みで応答を失った場合、呼出側の再試行で重複する可能性がある。これは自動リトライが現在あるという指摘ではなく、結果表現が再試行判断に不十分という設計上のリスク。

**修正方針:** 適用済みフィールド、未適用フィールド、結果不明、照合用IDを返す。書込みは結果照合なしで自動再試行しない。競合時に取得済みの古い値を再送して他者の変更を上書きする問題も、F04/F05の補完実装時に考慮する。

## データ・プロトコル・機能設計

### F11 [P2] 誤った追加引数を黙って無視し、通常コメント等の意図しない操作になる

箇所: `src/tools/pull_request.rs:82`ほか各Params。

多くのParamsはserdeの未知フィールドを拒否しない。**AddPullRequestCommentParamsへpath/line/commit_idを追加してもデシリアライズ成功し、情報が捨てられることを再現。**MCPクライアント側でスキーマ外引数を検証しない場合、インライン投稿を意図して通常コメントを作ることがある。

**修正方針:** Paramsに `deny_unknown_fields` を適用し、生成スキーマと実行時検証を一致させる。非対応の指定は操作前にエラーにする。

### F12 [P2] PRのdraft・ラベル・担当者、Issueのmilestone情報を返却時に落とす

箇所: `src/models/pull_request.rs:16`、`src/models/issue.rs:13`。

公式PR応答にはdraft/labels/assignees等があるが、モデルにないので捨てられる。draftを通常PRと区別できず、業務上の担当や分類も取得不能。Issueのmilestoneも同様。PRのmerged_byや各関連API URLも落ちる。APIが提供していない情報とは区別すべき。

**修正方針:** 実際のGitBucket応答をfixtureにして必要フィールドを保持する。nullableと未対応を必要に応じて区別する。

根拠: [公式PR型](https://github.com/gitbucket/gitbucket/blob/4.44.0/src/main/scala/gitbucket/core/api/ApiPullRequest.scala)、[公式Issue取得](https://github.com/gitbucket/gitbucket/blob/4.48.0/src/main/scala/gitbucket/core/controller/api/ApiIssueControllerBase.scala)。

### F13 [P2] マージ方法と「レビューしたSHAに限定する」指定がMCPからできない

箇所: `src/tools/pull_request.rs:70`、`src/tools/pull_request.rs:233`、`src/models/pull_request.rs:55`。

モデルにsha/merge_methodがあるがツールでは常にNone。squash等を選べず、レビュー後にheadが更新されても最新headをマージしてしまう可能性がある。

**API側の制約:** 公式4.44.0/4.48.0にはsha未実装のTODOが残っているため、MCPにsha入力を追加するだけでは条件付きマージを保証できない。事前GETによる比較にも競合窓が残る。原子的な保証にはGitBucket側の対応が必要。マージ方法は環境でサポートされる戦略を確認して公開する。

根拠: [公式マージ処理](https://github.com/gitbucket/gitbucket/blob/4.48.0/src/main/scala/gitbucket/core/controller/api/ApiPullRequestControllerBase.scala)。

### F14 [P2] 実際のクローズ日時でない値をclosed_atとして返す

箇所: `src/api/issue.rs:192`。

closed_atがないとupdated_atを代入する。クローズ後に本文やラベル等を変更したIssueでは、クローズ日時が後の更新日時に変わる。リードタイム集計や期限判定で誤る。READMEに注記はあるが、機械利用するレスポンスだけでは推定と分からない。

**修正方針:** 不明はnullのまま返す。必要なら別名の推定値と出典を付ける。実際の日時を得るには状態変更イベントの取得が必要だが、通常コメントAPIだけでは全イベントを取れない。

### F15 [P2] 本文・コメントのtrimでMarkdownの意味が変わる

箇所: `src/tools/validation.rs:5`、`:29`、`src/tools/issue.rs:228`、`src/tools/pull_request.rs:255`。

本文やコメントもtrimして送る。例えば最初の行が4スペースで始まるインデントコードブロックはコードでなくなり、末尾の空白や改行も失われる。空白だけの入力チェックと、原文を書き換えることは別の処理。

**修正方針:** owner等の識別子は正規化しても、本文は空白のみかを検証して原文を保持する。Markdown/コードを含む回帰ケースを追加する。

### F16 [P2] 一覧を全件・全本文で返す設計がMCP利用に向かない

箇所: `src/api/client.rs:204`、`src/tools/response.rs:32`、各listツール。

page/cursor/limit/fields/since等を指定できず、修正後は大量データを一度に返す。PRにはhead/baseのRepositoryもあり、応答が膨らむ。現状のページ上限は結果を部分返却せずエラーにするだけ。クライアントでの切詰めやコンテキスト消費により「一覧にない」という誤判断を招く設計上のリスクがある。

**修正方針:** 一覧は要約と明示的なページ/継続情報を返し、本文や詳細はgetで取得する。上限到達時は取得範囲を明示する。GitBucketの非ページングAPIに対してもMCP側の応答分割を設計する。

### F17 [P3] 読取り・破壊的操作のMCP注釈がない

箇所: `src/tools/*.rs` の `#[tool]`。

list/get、create/comment、delete/mergeの性質をannotationsで表現していない。クライアントの確認UIやツール選択が説明文依存になる。注釈は認可機構ではなく、クライアントにとってのヒントとして追加する。業務で閲覧専用にしたい場合のサーバー側read-only設定も現在ない。

## HTTP・認証・設定

### F18 [P2] owner/repo/usernameをURLへ直接埋め込み、対象パスが変化する

箇所: `src/api/repository.rs:23`、`src/api/user.rs:13`、Issue/PR/Web各URL、`src/tools/validation.rs:13`。

検証は空欄チェックのみで、URLはformat!で作る。`#`, `?`, `/`, `..`等を受け入れる。**repo=`r#different`で実際には`r`をGETして成功することを再現。**正規のGitBucket名にこれらが使えるという指摘ではなく、不正な入力を別の有効対象に変換して処理する問題。書込みルートでも安全なパス構築が必要。ラベル名だけはpath_segments_mutを使っている。

**修正方針:** owner/repo/usernameをGitBucketの名前規則で検証し、全APIをパスセグメントとして構築する。base URLもquery/fragment/userinfoを無言で捨てず、許容する形式を明示する。

### F19 [P2] HTTPリクエストのタイムアウトがない

箇所: `src/api/client.rs:61`、`src/api/web.rs:21`。

connect/total/read timeoutを設定していない。応答しないサーバーや接続途中で止まるプロキシにより、ツール実行やWebログインが長時間終わらない。複数ページやフォールバックで待ち時間も増える。

**修正方針:** 接続・各HTTP要求・一連のツール実行に時間予算を設定し、timeoutをエラー種別として返す。書込みtimeoutは「未適用」と断定せずF10の結果不明として扱う。

### F20 [P2] RESTトークンとWebログインの実行ユーザーが一致する保証がない

箇所: `src/api/client.rs:37`、`src/api/issue.rs:162`、`src/api/web.rs:13`。

トークンがalice、Web資格情報がbobでも受け付ける。通常はaliceで操作し、特定の更新だけbobの権限・監査名になる。データの可視性も異なり、Webで変更した後にトークンで結果を取得できないケースもある。環境設定に依存する設計上のリスク。

**修正方針:** 認証主体の一致を確認するか、意図的な代理実行設定として明示する。ログ/結果に実行経路と主体を残す。ログイン成功を単なる遷移先URLだけで判定せず、認証済み主体を確認する。

### F21 [P2] 資格情報がDebug・設定エラーに露出し得る

箇所: `src/config.rs:8`、`:22`、`:66`、`src/api/client.rs:10`、`:18`、`src/main.rs:20`。

Config/ConfigFile/WebCredentialsがDebugをderiveし、password/tokenをそのまま出力できる。**GitBucketClientのDebug文字列にWebパスワードが入ることを再現。**現在の通常ログが常に全資格情報を出すという意味ではないが、デバッグ追加で漏れやすい。TOMLのparseエラー表示も不正行のソース抜粋を含み得るため、token行に構文ミスがあればstderrへ秘密が出る経路がある。起動時には設定のURLもそのまま表示するためuserinfo等を含める運用にも注意が必要。

**修正方針:** 秘密型または伏せ字のDebugを実装する。設定エラーはソース本文を含めず、位置と原因だけ返す。起動ログのURLをサニタイズする。

### F22 [P2] 設定ファイル0600の保証が不完全

箇所: `src/config.rs:124`、READMEのConfiguration。

OpenOptions.mode(0600)は新規作成時にだけ効く。**0644の既存ファイルへsave_toしても0644のままになることを再現。**loadでも権限を検査しない。また現在の実行バイナリは設定を自動作成・保存せず、READMEの手動作成手順にもchmodがないため「config file is created with 0600」という記述は通常の手動セットアップを保護しない。

**修正方針:** 保存時は既存ファイルの権限も明示的に制限する。手動作成手順にumask/chmodを含める。保存は一時ファイル→置換で原子化し、既存symlinkを追跡して別ファイルをtruncateする問題も避ける。save_toは現在のmainから未使用なので、保存経路の問題と実運用の手動ファイル問題は分けて扱う。

### F23 [P2] 日付が検証されず、Unicode文字列でpanicする

箇所: `src/api/milestone.rs:280`、`src/tools/milestone.rs:154`、`:178`。

日付をStringで受け、Web用変換でbyte長と5バイト目だけを見て `trimmed[..10]` を取る。**`2026-あいう`をdue_onとして送ったフォールバック経路で文字境界panicを再現。**通常のinvalid dateではなくタスクがpanicし、通常の構造化エラー処理を通らない。mainの全プロセスが必ず終了することまでは確認していない。

**修正方針:** typed date parserで操作前に検証し、UTF-8文字列を固定byte indexで切らない。

### F24 [P2] 日付形式・削除指定がRESTとWebで統一されていない

箇所: `src/models/milestone.rs:32`、`src/tools/milestone.rs:47`、`src/api/milestone.rs:146`、`:280`。

ツールは日付形式を示さず文字列をそのままRESTへ送る。公式JSON日付はInstantとして解析するので、`2026-04-01`や更新時の空文字はRESTの日付値にならない。一方Webは日付だけを使い、T以降を捨てる。API/入力によって失敗、Web救済、日付情報の切捨てが変わる。`to_milestone_form_due_date`はResultを返すが実際には形式エラーを返さない。

**修正方針:** 外部入力は日付かRFC3339かを決めて文書化し、RESTとWebそれぞれへ型付きで変換する。省略/削除/nullを区別し、無効な日付やタイムゾーン付き値をテストする。

根拠: [JSON日付の公式パーサ](https://github.com/gitbucket/gitbucket/blob/4.48.0/src/main/scala/gitbucket/core/api/JsonFormat.scala#L17)。

### F25 [P2] APIのエラー本文・サイズを無制限でそのまま扱う

箇所: `src/api/client.rs:171`、`:243`、`:253`、`src/api/web.rs:248`、`src/tools/response.rs:51`。

成功・エラーともレスポンス全体をメモリへ読み込み、エラー本文はMCPへ無加工で返す。プロキシのHTMLページや巨大なスタックトレースが会話へ入る。HTMLに内部情報があるとそのまま公開される条件付きリスクがある。またログ上はHTTP 200を「成功」と出した後に、ラップされた404やJSON失敗となる場合があり、診断しにくい。

**修正方針:** サイズ上限、Content-Type確認、JSONエラーの要約、HTMLの短い要約、機密値の除去を行う。HTTP状態とアプリケーション解析結果を分けて記録する。

### F26 [P3] 完全な環境変数設定があっても壊れた設定ファイルで起動できない

箇所: `src/config.rs:172`、`:182`。

環境変数の解決前に必ずTOMLを読んで検証する。URL/tokenが両方envにあっても、古いusername/password入りファイル、不正TOML、読取り不能ファイルがあると失敗する。「環境変数優先」を期待する運用では意外な挙動。ファイル検証を常に必須とする方針自体はあり得るが、現在の説明と整合していない。

**修正方針:** 完全なenv設定ならファイルを不要にするか、厳格検証方針を明記する。既存のenv-only web資格情報方針を維持して移行案内を返す。

## テスト・運用・文書

### F27 [P1] テストのAPI契約が実GitBucketと異なり、重要な問題を検知できない

箇所: `tests/api_client_test.rs:159`、`:630`、`:932`、`:1381`、`:1485`、`tests/e2e_test.rs:729`。

- ページングテストは100件のページを自作するが、Issue/PRは25件固定、コメント/リポジトリ一覧はpageを使わない。
- PR更新テストは期待するJSONを返すだけで、title/bodyの欠落に対するGitBucket実装の挙動を再現しない。
- マイルストーンE2Eはtitle/state等をまとめて変更し、省略したフィールドの保持を検証しない。期日保持・クリアも不足。
- ラベルE2Eは名前と色を同時変更し、色だけの422を通らない。
- インラインコメントのfixture/テストがなく、未対応が明示されていない。
- 通常cargo testでは実サーバー用17件は実行されない。Dockerは4.44.0固定で、実業務のバージョン差も分からない。

**修正方針:** まず公式応答・契約からfixtureを作り直す。Issue/PR 26件、非ページング一覧100件、PR title-only/body-only、閉じたmilestoneのtitle-only、label color-only、HTML入力エラー、異なる権限ユーザーを最低限の回帰セットにする。少なくとも正式対応の最古/最新バージョンで契約テストを実施する。

### F28 [P2] Rust 1.70+というREADMEの要件は成立しない

箇所: `README.md:22`、`Cargo.toml`、`Cargo.lock`。

ロックされているrmcp/rmcp-macros 1.3.0はedition 2024、schemars 1.2.1はMSRV 1.74、darling 0.23.0は1.88を要求する。自コードにもis_some_and等がある。Rust 1.70でビルドできる説明にはなっていない。Cargo.tomlにrust-versionがなく、CIはstableのみ。

**修正方針:** 依存解決込みで最低対応Rustを決め、rust-versionとREADMEを一致させ、そのバージョンをCIで検証する。

### F29 [P3] --helpと単体起動時readyの検証手順が実装に合っていない

箇所: `VERIFICATION.md:11`、`:41`、`src/main.rs:10`、`:41`。

CLI引数を解析しないため `--help` も通常のMCPサーバー起動になる。設定なしでは設定エラー、設定ありではMCP入力待ちになり得る。readyはserveの初期化後に出るため、MCP初期化を送らない単体ターミナル起動で文書どおりreadyになるとは限らない。

**修正方針:** --help/--version/設定診断を実装するか、文書を実際の動作へ合わせる。プロセス起動・MCP初期化済み・GitBucket接続確認を別々の状態として説明する。

### F30 [P2] E2E用資格情報ファイルとDocker公開範囲の保護がない

箇所: `scripts/e2e/bootstrap.sh:174`、`scripts/e2e/common.sh:48`、`docker/e2e/compose.yaml:5`。

runtime.envへトークン/パスワードを平文で書くが、umask/chmodがなく一般的なumaskでは他ユーザーから読める。export行への値はshellエスケープもなく、カスタム資格情報に空白/特殊文字があるとsource時に壊れる。Composeはhost IPを省略しているためローカル限定にならず、root/rootの検証インスタンスが他ホストから到達可能な環境もある。業務サーバーではなく検証環境に限定した問題。

**修正方針:** runtimeディレクトリ0700、ファイル0600、正しいshell quoting、既定ポート127.0.0.1限定にする。失敗時も一時cookie/tokenファイルをtrapで片付ける。

## 不具合とは分けて扱うべき機能不足・API制約

| 領域 | 現状と制約 | 対応方針 |
| --- | --- | --- |
| PRの差分・変更ファイル・コミット | MCPにツールがない。公式RESTのPR commitsはあるがfilesルートは確認版では未実装 | commitsを公開し、差分はGit/Web/プラグインのどれを使うか設計する |
| PRの承認・変更要求・レビュー状態・解決済みスレッド | このMCPに機能なし。GitHubのreview/resolve APIをそのまま使える前提は置けない | GitBucket本体/プラグインが持つ概念を調べて専用設計 |
| コメントの取得・編集・削除 | 一覧/作成だけ。通常コメントの単体GET/PATCH/DELETEは公式RESTにある | コメントの種類別にツール追加。誤投稿の訂正にも必要 |
| Issue/PRへのラベル付け替え | リポジトリのラベル定義CRUDはあるが、既存Issue/PRへの付与・除去ツールなし | 公式Issue-label APIの追加。ラベル定義変更と区別 |
| Issue/PRの担当者・milestone | Issue作成時のassigneesだけ。更新やmilestone指定がない | 公開可能なAPI/Web機能を確認し、読取りモデルから整備 |
| Draft PR | 公式作成APIにdraft指定はあるが、MCP入力・出力にない | draftを作成・表示できるようにする。状態変更は別途API確認 |
| 組織リポジトリ作成 | 公式RESTにorgs/{org}/reposがあるが、MCPは認証ユーザーへの作成のみ | 必要ならowner/organizationを入力として追加 |
| 閲覧可能な全リポジトリ | MCPはowner必須。公式user/reposを公開していない | 自分がアクセスできるリポジトリの探索ツールを追加 |
| フォーク元リポジトリ名が異なるPR | 確認版の公式REST作成はhead ownerと対象repoと同じ名前でリポジトリを探す | 改名forkからのPRはAPI制約。Web/API拡張を検討 |
| mergeable | 公式ApiPullRequestはNoneを返す実装であり、常時不明になり得る | nullを「競合なし」に読み替えず、不明として表示 |
| PATとWeb認証 | PATだけでは通常のWebフォームセッションにならない。SSO/OIDC専用等ではusername/password signinが使えない可能性 | 認証方式別の対応表と、RESTだけで使える範囲を公開 |
| 信頼CA/TLS | mainはallow_invalid_certs=false固定。E2Eのinsecure設定は本番バイナリではない | 社内CAの配布・信頼設定を文書化。必要ならCA追加設定を設ける |

不足機能はすべて既存仕様のバグとは限らない。ただしPRレビューをMCPだけで完結させたい用途に対しては、重要な不足である。

## 追補: Webフォールバックを本当に必要とする操作

依頼を受けて、現在フォールバックを持つ操作について再確認した。**「フォールバックが実装されている」ことと「その操作のREST APIがない」ことは一致しない。** 現在のコードも原則RESTを先に試し、404時にだけWebへ進む。問題は、RESTへの不適切な入力による404も「API未提供」と解釈している点にある。

以下は公式4.44.0/4.48.0のソースでの判定。業務インスタンスでの実行検証ではなく、旧版・プラグイン・リバースプロキシでの差異は別途確認が必要。既存のWeb互換処理を一律に削除するという提案ではない。

### 操作ごとの判定表

| 現在Webへ進む可能性がある操作 | 公式RESTの有無 | Webを不要にできる範囲・残る条件 |
| --- | --- | --- |
| マイルストーン作成 | `POST /repos/{owner}/{repo}/milestones`あり | RESTで許容されるtitleと適正な日時なら不要。日付の形式変換で救済できる場合がある。日本語・空白入りtitle等はREST側の制約があり、Webを残す理由になる |
| マイルストーンの説明・期日・タイトル更新 | `PATCH /repos/{owner}/{repo}/milestones/{number}`あり | 現在値をGETし、必須titleと維持すべきstate/description/due_onを補完すれば、多くの部分更新で不要。現在title自体がREST検証に通らない場合は例外 |
| マイルストーンを閉じる・再度開く | 同PATCHが`state`を処理する | stateだけの要求を、現在のtitle/description/due_onを含む完全な入力へ変換すれば不要。REST非対応ではなく、現在のリクエストのtitle不足でWebへ進むケースがある |
| マイルストーン削除 | `DELETE /repos/{owner}/{repo}/milestones/{number}`あり | 確認した公式版では不要。titleの文字種にも依存しない。404が出る環境は、まず版・経路・権限・エラー包装を調べる |
| PRのタイトル・本文・状態・base更新 | `PATCH /repos/{owner}/{repo}/pulls/{id}`あり | F04のtitle/body補完と反映検証を行えば、通常は不要。PR→Issue→Webという現在の互換経路は、PR PATCHが実際に使えない環境向けに限定する |
| ラベルの名前変更、または名前と色の同時変更 | `PATCH /repos/{owner}/{repo}/labels/{name}`あり | 変更後nameとcolorを送れば通常不要。コードは既に`new_name`をRESTの`name`へ変換している。F06の同名更新制約とは区別する |
| ラベルの色だけの変更 | 同PATCHは存在するが、同名を重複扱いする | 確認版ではAPI実装の制約により、入力補完だけでWeb不要にはできない。既知ケースを識別してWebを使うか、GitBucket側を修正する |
| Issueのタイトル・本文更新 | `PATCH /repos/{owner}/{repo}/issues/{id}`は確認版で未実装 | 通常のIssueについてはWebまたはAPI拡張が必要。対象がPRならIssue更新の代わりにPR PATCHを使用できる |
| Issueのクローズ・再オープンだけ | Issue PATCHは未実装。ただしコメントPOSTに`action`処理がある | 実際のコメント投稿を伴うならRESTで可能。コメントなしの状態変更と同等ではないため、無条件な置換はしない |
| ラベルdescription | 標準の入力型にフィールドなし | RESTでも現在のWebフォームでも保存できない。Webを追加して解決する種類の問題ではない |

PRインラインコメントは現状のWebフォールバックには含まれていない。今後実装する場合のREST制約はF01のとおりで、上表のPR通常更新とは別に判断する。

### 1. 最優先で不要なWeb依存を減らせるのはマイルストーン

関連: F05/F09/F24。箇所: `src/api/milestone.rs:50`、`src/models/milestone.rs:42`。

例えば現在値がtitle=`v1.0`、description=`Release notes`、期日ありの場合、MCPの「閉じる」要求を現在は次のように送る。

```json
{"state":"closed"}
```

しかし公式PATCHの入力型はtitle必須なので、これが抽出できず404になり、コードはWeb資格情報を要求する。GETした値を保った次の入力なら、公式RESTに状態変更の処理がある。

```json
{
  "title":"v1.0",
  "state":"closed",
  "description":"Release notes",
  "due_on":"2026-12-01T00:00:00Z"
}
```

説明だけ・期日だけの更新も同じ考え方で変換する。期日クリアはRESTへ空文字を送るのではなくnull等のOptionなしとして表現する必要がある。クライアント側は「入力省略＝保持」「明示クリア＝削除」を区別したリクエスト型を用意する。

**ただしタイトル制約はクライアント修正だけでは解消しない。** 確認版のRESTは100文字以内かつ英数字・`-+_.`のみを許す。Webは同じ制約ではないため、Webで作成した`第1四半期 リリース`等の既存マイルストーンは、state変更のためにtitleを補完してもRESTの検証に通らない。この場合のWeb利用は妥当。勝手なtitleの変更で回避しない。

削除はtitleを入力しない独立したDELETEルートなので、この制約はない。マイルストーンの作成・更新・削除をまとめて「REST非対応」と案内する現在のエラー文は修正すべき。

根拠: [4.44.0のマイルストーンAPI](https://github.com/gitbucket/gitbucket/blob/4.44.0/src/main/scala/gitbucket/core/controller/api/ApiIssueMilestoneControllerBase.scala)、[4.48.0の同API](https://github.com/gitbucket/gitbucket/blob/4.48.0/src/main/scala/gitbucket/core/controller/api/ApiIssueMilestoneControllerBase.scala)、[4.48.0の必須項目・タイトル検証](https://github.com/gitbucket/gitbucket/blob/4.48.0/src/main/scala/gitbucket/core/api/CreateAMilestone.scala)、[Webの入力検証](https://github.com/gitbucket/gitbucket/blob/4.44.0/src/main/scala/gitbucket/core/controller/MilestonesController.scala#L34)。

### 2. PR更新はIssue更新のWeb経路に寄せる必要がない

関連: F04。箇所: `src/api/pull_request.rs:50`、`:102`、`:120`。

確認した両版にPR PATCHがある。タイトル/本文の片方だけの変更は現在値を補完して専用PATCHへ送り、状態とbaseは専用フィールドを使う。PRのタイトル・本文・状態を変更するために、未実装のIssue PATCHへ迂回する必要は通常ない。

ただし「RESTがあるから今のコードのままでよい」という意味ではない。F04の消失/未反映を先に修正し、応答値を照合する。PR PATCHへの404でPR自体がGETできても、API未実装とは確定しない。旧版向け互換経路を残す場合は、対象版や能力を識別して使う。

根拠: [PR PATCHルート](https://github.com/gitbucket/gitbucket/blob/4.48.0/src/main/scala/gitbucket/core/controller/api/ApiPullRequestControllerBase.scala)、[更新処理の注意点](https://github.com/gitbucket/gitbucket/blob/4.48.0/src/main/scala/gitbucket/core/service/PullRequestService.scala#L309)。

### 3. ラベル更新を一括で「API未実装」と扱うのは不正確

関連: F06/F07。箇所: `src/api/label.rs:35`、`:73`、`:94`。

両版にPATCHはあり、別の未使用名への改名と色変更はRESTで処理できる。一方、同じnameを送る色だけの変更は422になる。これはルートの不在とは原因が異なり、Webの必要性も異なる。

既存README/TESTING.mdの「公式4.44.0でラベル更新RESTが未対応なのでWebへ」という説明は、公式ソースからは一般化できない。過去のE2E環境で404が発生したことと、公式ルートが存在しないことを分け、使用イメージの実体、入力、レスポンス包装、プロキシ等を再調査する。404という理由だけで名前変更までWeb依存にする必要はない。

なお色変更をRESTだけで行うための「一時名へ改名→元の名へ改名」や「削除→再作成」は推奨しない。途中状態や失敗、参照・ID・監査情報への影響があり、単純な色更新と同等ではない。

根拠: [4.44.0のラベルPATCH](https://github.com/gitbucket/gitbucket/blob/4.44.0/src/main/scala/gitbucket/core/controller/api/ApiIssueLabelControllerBase.scala#L72)、[4.48.0の同実装](https://github.com/gitbucket/gitbucket/blob/4.48.0/src/main/scala/gitbucket/core/controller/api/ApiIssueLabelControllerBase.scala#L72)。

### 4. Issueの状態変更には、条件付きで使えるREST経路がある

箇所: `src/api/issue.rs:45`、`:83`。この代替経路は現在のMCPでは公開していない。

公式の `POST /repos/{owner}/{repo}/issues/{id}/comments` は、JSONのbodyとは別にリクエストパラメータの`action=close` / `action=reopen`を解釈する。クローズ/再オープンと同時に実際のコメントを投稿する要件なら、Webセッションなしで処理できる可能性がある。権限はGitBucket側で確認される。

ただし空のbodyは拒否される。ダミーコメントを生成して状態だけの変更を代替すると、通知・監査・会話履歴が変わる。したがって`update_issue(state=...)`全般のWebフォールバックをこの方法で置換してよいとは言えない。「コメントして閉じる/再オープンする」という独立した操作として提供し、実環境の契約テストを行うのが適切。

根拠: [コメントPOSTのaction処理](https://github.com/gitbucket/gitbucket/blob/4.48.0/src/main/scala/gitbucket/core/controller/api/ApiIssueCommentControllerBase.scala#L136)。

### 修正方針への反映

1. マイルストーンのRESTリクエスト補完・日付変換を実装し、不必要なWeb資格情報要求を減らす。
2. PR更新は専用RESTで完結させ、通常Issueと別の能力として管理する。
3. ラベル改名・同名の色更新・descriptionを別々に判定する。
4. Webを使用する理由を「APIルート不在」「APIの入力制約」「既知のAPI実装不具合」に分ける。単なる入力不正や権限エラーを自動的にWebへ流さない。
5. README/TESTING.mdとエラー文に、対象GitBucket版・条件・代替経路を明記する。F01/F05/F06/F09の指摘と合わせて対応する。

## 推奨する修正順序

1. **データ保持と一覧の正確性:** F02/F03/F04/F05を修正し、実契約の回帰テストを先に作る。PRタイトル単独更新とマイルストーン単独更新は特に優先。
2. **インラインの専用設計:** F01/F11/F12を対応。まず正確に読み取れるようにし、投稿時のコミットと旧行/新行の指定を実装する。利用中のGitBucketバージョン/プラグイン/認証を確定し、Web方式かAPI追加方式を選ぶ。
3. **更新の成功判定:** F06〜F10を対応。非対応/入力不正/部分適用/結果不明をレスポンスで分離する。
4. **安全性と運用:** timeout、日付panic、秘密情報、設定権限、URL構築を修正する。
5. **機能拡張と文書整合:** 差分、draft、担当者/ラベル/milestone更新、マージ戦略、ページ付き出力、対応バージョン表を整備する。

設計上は、GitHub互換を仮定したモデルをそのままRESTに送る層と、Webフォールバックが混在している点が問題の中心にある。API層にバージョン/能力ごとのGitBucketアダプタを置き、ツール層には「何を要求し、何が適用されたか」を安定した形式で返す構成が望ましい。現状のGitBucketApi traitはその分離の足場として利用できる。
