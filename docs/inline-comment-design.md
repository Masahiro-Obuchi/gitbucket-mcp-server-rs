# PRインラインコメントの取得・投稿設計（P08）

P08は契約調査と設計の単位である。現在のMCPサーバーにインライン用ツールを
追加したものではない。基点はPR #28マージ後の `75536dd`。

## 採用する経路と到達範囲

**P09/P10では、標準Web経路を範囲付きで実装する。原文・全履歴・原子的なhead制約を
必須とする要求は、その能力を持つ拡張APIがなければ非対応とする。**
Web経路を調べた結果、閲覧者が他者のMarkdown原文を取得できず、履歴の置換による
コメント欠落もある。HTMLからMarkdownを復元して完全対応とすることはできない。

| 業務上の要求 | 標準Webでの方針 | 完了に必要なもの |
| --- | --- | --- |
| 画面から取得できるコメントの参照 | P09で実装可能。HTML・取得範囲・不足理由を明示 | DOM解析、専用型、完全性情報、認証・構造変更テスト |
| 閲覧権限だけで他者の原文を取得 | 標準Webでは不可。権限昇格や編集権限の要求で代替しない | 読取り認可で原文を返す拡張API |
| 明示的にPRへ関連付いた全コメントの履歴 | 会話画面だけでは不可 | PR関連付けを保存元で検索する拡張API |
| 現行差分の単一行へ投稿 | P10で制約付き実装可能 | 差分・commit・path・行の検証、事前head照合、投稿後照合 |
| 前後空白を含む本文をそのまま投稿 | Webフォームがtrimするため事前に拒否 | 原文を保持する拡張API |
| 原子的なexpected head制約 | Webにはない。事前照合の競合窓を明示 | Git更新と協調するサーバー側の排他・条件検証 |
| GitHub型の返信・解決状態 | 確認した保存モデル・フォームにはない | 別機能としての上流対応。架空の値を返さない |

拡張APIの実装・プラグインの導入は本PRに含めない。業務環境へ追加のプラグインを
導入できるとは仮定せず、P09/P10のWeb実装と分けて扱う。

## 根拠と検証環境

公式4.44.0・4.48.0のソースと、両版の使い捨てDocker環境を調べた。
ファイル別のGit blob・URLは
[sources.json](../tests/fixtures/inline_comments/sources.json)、
実行日時・image digest・観測値は
[4.44.0](../tests/fixtures/inline_comments/4.44.0/environment.json) /
[4.48.0](../tests/fixtures/inline_comments/4.48.0/environment.json) に記録した。
標準同梱イメージを使用し、追加プラグインは導入していない。コンテキストパスは `/`、
HTTPはloopback限定。所有者PATと、所有者・GUESTの別々のWebセッションを用いた。
非公開リポジトリに `permission=pull` でGUESTを追加し、権限応答も記録した。

再現手順は[TESTING.md](../TESTING.md#inline-comment-contract-probe-p08)、
実行用コードは[probe_inline_comments.py](../scripts/e2e/probe_inline_comments.py)。
コミット・ユーザー・コメントはすべて調査専用に生成した。
ホスト名・ユーザー名・リポジトリ名・完全長commit SHAを正規化し、本文・行・保存IDは保持した。
fixtureは完全なHTTP録画ではなく、実応答のDOM断片とそこから抽出した観測値である。
Cookie、PAT、パスワード、認証ページ全体は保存していない。

## 保存元と取得経路

| 種類 | 保存元・関連付け | 標準の取得元 | IDの扱い |
| --- | --- | --- | --- |
| 通常のPR会話コメント | `IssueComment`、PRのissue ID | `GET /api/v3/repos/{owner}/{repo}/issues/{number}/comments` | `issue:<id>` |
| コミット全体コメント | `CommitComment`、commit、fileなし。issue IDは任意 | PR会話・commit画面 | `commit:<id>` |
| ファイル全体コメント | `CommitComment`、fileあり、旧/新行なし | PR会話・files・commit画面 | `commit:<id>` |
| 行コメント | `CommitComment`、file、旧/新行、commit。issue IDは任意 | PR会話・files・commit画面 | `commit:<id>` |

[Comment.scala](https://github.com/gitbucket/gitbucket/blob/4.48.0/src/main/scala/gitbucket/core/model/Comment.scala)
では、現在のcommit/旧行/新行と、`originalCommitId` / `originalOldLine` /
`originalNewLine` が別に保存される。更新日時も保存元にはある。
これらがすべてHTMLに公開されるわけではない。

[IssueコメントAPI](https://github.com/gitbucket/gitbucket/blob/4.48.0/src/main/scala/gitbucket/core/controller/api/ApiIssueCommentControllerBase.scala)
は通常コメント等を返す。調査用の通常コメントIDは1で、APIから取得できた。
同APIの一覧には11件のCommitCommentは混入しなかった。
所有者PATで `GET /pulls/1/comments` と `GET /commits/{HEAD1}/comments` は両版とも404。
[PRコントローラ](https://github.com/gitbucket/gitbucket/blob/4.48.0/src/main/scala/gitbucket/core/controller/api/ApiPullRequestControllerBase.scala)
およびcommitコントローラのソース確認と合わせ、確認版の標準RESTには目的のCRUD経路がないと判断した。
未知の業務環境で404だけから非対応を推定するものではない。
`review_comments_url` やWebhook用の型の存在も、その経路の実装を証明しない。

## Webから得られる情報と欠落条件

### 本文と認可

会話・files画面はレンダリング済みHTMLを返す。
`div.commit-commentContent-{id}` をHTML本文として保持し、Markdownとは区別する。
`**bold**` と `&` / `<tag>` の投稿例から、装飾・サニタイズされたHTMLを原文と同一視できない。

`GET /{owner}/{repo}/commit_comments/_data/{id}?dataType=html` は
編集用textareaを返す経路である。
[Webコントローラ](https://github.com/gitbucket/gitbucket/blob/4.48.0/src/main/scala/gitbucket/core/controller/RepositoryViewerController.scala)
は編集可能性を検査する。所有者では200と保存後の原文、他者コメントを読むGUESTでは401となった。
GUESTの通常の会話・files画面は200だった。既に持つ権限で取得できた原文だけを原文として扱い、
取得できない場合は `body_format=html`、`body_raw=null` とする。
P09の既定読取りで編集用経路を必須にしない。

投稿フォームのcontentはtrimされる。要求した
`"    indented code\n\n日本語 & <tag> **bold**  \n"` に対し、保存後のtextareaは
`"indented code\n\n日本語 & <tag> **bold**"` だった。復元できるのは保存後の本文であり、
投稿前に失われた空白は回復できない。

### IDとDOMの種類

行/ファイルのカードは `.commit-comment-box#discussion_r{id}` にfilename・oldline・newlineを持つ。
コミット全体のカードは `.commit-comment-box.commit-comment-{id}` という別形で、id属性がない。
後者を無視するセレクタでは欠落する。著者はusername要素、登録日時の表示値は日時要素から得られる。
タイムゾーンや精度を確定できない表示から厳密な日時を合成しない。
更新日時・元commit/元行を同じDOMから得られるとは限らない。

調査例では通常コメントのID1とコミット全体コメントのID1が共存した。
両版の[会話統合](https://github.com/gitbucket/gitbucket/blob/4.48.0/src/main/scala/gitbucket/core/service/PullRequestService.scala)
は両者を同じグループキーに入れ、会話画面ではコミット全体コメントだけが残った。
通常コメントはRESTから引き続き取得できる。保存元を含めたIDで扱い、
Web画面のID集合を保存済みコメント全体の集合としない。

### 位置・head変更・折りたたみ・履歴

正規化した実験は、BASEからFIRST、HEAD1の2コミットでPRを作り、HEAD2を追加後、
BASEから別のHEAD3へlease付きで調査用ブランチを置換し、そのPRをマージした。
既存ブランチや業務データにforce pushは行っていない。
両版の結果は同じだった。

| 例 | 初期位置 | HEAD2追加後のHTML | 履歴置換後 |
| --- | --- | --- | --- |
| ID3/4、同じ新行への2投稿 | review.txt 新2行 | 新3行、両IDを保持 | 会話/filesから欠落 |
| ID5、旧行 | review.txt 旧2行 | 旧2行 | 表示が残る |
| ID6、削除ファイル | deleted.txt 旧1行 | 新1行へ変化 | 表示が残る |
| ID7、改名後のファイル | renamed_日本語.txt 新7行 | 旧7行へ変化 | 表示が残る |
| ID9、FIRSTへの投稿 | review.txt 新2行 | 現行行として扱えないカード | 会話/filesから欠落、FIRST画面には残る |
| ID2、ファイル全体 | review.txt、行なし | `fold-comments-2` がdisplay:none | 会話/filesから欠落 |

この旧/新の反転は上流の位置更新結果の観測であり、正しい現在位置の証明ではない。
HTMLの属性は観測値として保持し、現在差分との照合なしに現行位置へ昇格しない。
折りたたまれたカードもレスポンスHTMLには含まれる。可視DOMだけに限定せず取得する。
マージ後も、履歴置換で欠落したコメントは会話/filesへ戻らなかった。

会話取得は現在のPR比較に含まれるcommit群のコメントを集める実装であり、
`issueId=PR番号` の保存行を全件検索する実装ではない。
さらにソース上、同一commitを共有する他PRのコメントも取り込む可能性がある。
後者はソースからの推論で、この実験では別PR間の混入を検証していない。
HTMLの表示だけからPRへの保存上の関連付けを確定しない。

## 投稿フォームの契約

| 項目 | 意味と方針 |
| --- | --- |
| URLのcommit | `POST /{owner}/{repo}/commit/{sha}/comment/_data/new`。PR番号そのものではない |
| `fileName` | file path。パス区切り・日本語・空白・`&`を正しくURL/form符号化する |
| `oldLineNumber` / `newLineNumber` | 旧側/新側の行番号。最初のMCP入力では正の一方だけを許可 |
| `content` | 必須本文。標準Webはtrimするので、原文が変わる入力は投稿前に非対応とする |
| `issueId` | PRのissue ID。省略すると単独のcommitコメントになり得るため必ず検証して付与 |
| `diff` | UIが送るJSONの表示用差分スナップショット。対象commit/path/行の正当性を証明しない |

通常の `/comment/new` はredirect、Ajax用 `_data/new` は生成したカードを返す。
後者でIDを取得できた。GUESTにも投稿は許可されたが、これを編集権限と混同しない。
`diff` の要素はoldLine/newLine/type/textを持つ。
[commentform](https://github.com/gitbucket/gitbucket/blob/4.48.0/src/main/twirl/gitbucket/core/repo/commentform.scala.html)
は旧側/新側で値を組み立てる。GitHubのpositionやsideをそのまま送らない。

両版とも `not-in-diff.txt` の新99999行という要求を受理し、ID11を返した。
成功HTMLは位置検証の代用にならない。
同じ位置への複数投稿も独立IDになる。画面のReplyは同じ位置へ新規投稿する仕組みで、
保存モデルにreply_toやresolvedはない。pathと元行で作られた表示グループを永続スレッドIDにしない。

## P09のモデルと読取り実装

通常コメント用の既存ツール・型は分離して維持する。
`list_pull_request_review_comments` と `get_pull_request_review_comment` を追加する。
新しいReviewCommentには次を持たせる。これは提案であり、現在の公開スキーマではない。

| フィールド | 意味 |
| --- | --- |
| `id`, `storage_id`, `kind` | `commit:<id>` と元の数値ID、commit/file/lineの種類 |
| `body_raw`, `body_html`, `body_format` | 確認できた保存原文とHTML。HTMLから原文を合成しない |
| `author`, `created_at`, `updated_at`, `url` | 各取得元で確認できた値のみ。更新日時不明はnull |
| `path`, `observed_old_line`, `observed_new_line` | HTML/APIから得た位置と出典 |
| `current_commit`, `original_commit`, `original_old_line`, `original_new_line` | 独立した値。不明はnull |
| `position_verified`, `source`, `association` | 現行差分との照合有無、取得元、保存上のPR関連付けの確認状態 |

一覧は `comments` と別に `coverage` を返す。少なくともscope、complete、
body_raw_complete、historical_complete、observed_head、missing_reasonsを表す。
Webのscopeは現在のPRページから得られる範囲で、PRの全履歴に対してcomplete=trueを返さない。
単一取得もPRページで所属を確認できないIDを別PRのものとして返してはならず、
未知/範囲外/権限不足を区別する。API全履歴能力がなければ欠落と削除を断定しない。

実装境界は、`src/models/` に共通型、`src/api/` に読取り操作サービスとWeb解析、
`src/tools/` に入力・出力とする。GitBucketApi・MockApi・MCP router・閲覧専用許可リストと
注釈を同じPRで更新する。HTTP予算・サイズ制限・安全なURL・PAT/Web主体照合を再利用する。
サインイン画面、権限エラー、想定外DOM、応答途中の制限到達を空の成功一覧に変換しない。
HTMLの既知構造を確認できる空一覧と構造破損を区別する。

## P10の書込み手順と制約

`add_pull_request_review_comment` を専用の変更ツールとして追加する。

1. owner/repo/PR、本文、commit、path、旧/新行の一方を検証する。未知引数、空白だけの本文、
   Webがtrimする前後空白、厳密なhead保証を要求する入力は、送信前に拒否する。
2. PRのhead/baseと差分を取得し、commitが対象PRに属し、pathと行がその差分に存在することを確認する。
   まず現在のPR差分・単一行だけを対象にし、削除行は旧側、追加行は新側で検証する。
   差分切詰め・binary・解析不能・改名対応を確認できないケースは非対応にする。
3. 認証主体を確認し、投稿直前にheadを再取得する。変化が見えればconflictとして書込みを止める。
   WebではこのGET後の競合窓が残ることを要求・能力情報で明示し、原子的保証と呼ばない。
   Webの非原子的な事前照合は入力上の明示選択を必須とし、atomicの要求を受け付けない。
   具体的な引数名はP10で確定する。
4. 検証したパス/行/commitからフォームを作り、一度だけPOSTする。通常コメントAPIへ代替投稿しない。
   redirect再送、timeout再送、認証更新後の自動再投稿を行わない。
5. 応答IDを抽出し、対象PR内の再取得結果で本文・位置・著者・関連を照合する。
   Webで保存上の関連付けまで確認できない範囲は結果にも残す。
   保存済みの可能性があるのにID抽出・応答読取り・事後照合が失敗したら、既存OperationOutcomeで
   unknownと観測済みID/対象/経路を返す。照合できた項目だけを確認済みとし、再投稿しない。

P09で投稿後の照合に必要な情報を確保できるまで、P10を使えると宣言しない。
過去commit・ファイル全体・編集/削除・返信・解決は初期公開から除外し、必要なら別の単位で設計する。

## 必要な場合の拡張API最小仕様（未実装）

提案する名前空間は `/api/v3/gitbucket-mcp/v1/repos/{owner}/{repo}/pulls/{number}/review-comments`。
標準GitBucketにこの経路が存在するという意味ではない。

- GET一覧・GET単一はリポジトリの読取り認可で保存原文と現在/元位置、著者、両日時を返す。
  `issueId=対象PR` のCommitCommentを現在のGit比較と独立に取得し、他PRの行を混ぜない。
  単独commitコメントは別scopeとし、現在のPR commit集合で確認できた関連だけを示す。
  過去に外れたcommitとPRの関係は既存保存行だけでは復元できないため、全履歴の保証範囲を
  明示的なPR関連行に限定する。必要なら以後のcommit履歴を保存し、記録開始時点を返す。
- レスポンスには能力情報、scope、完全性、観測head、継続cursorを含める。
  cursorは対象・条件・snapshot/revisionに束縛し、変化を検出したら再取得条件を返す。
  原文・元位置・履歴について、nullと非対応の理由を区別する。
- POSTは原文を保持し、空白だけの本文の禁止と書換えを分ける。expected_head_sha、commit、
  path、旧/新行の一方を検証し、PR関連と位置をサーバーで確認してから保存する。
  expected_head_sha不一致は副作用前の409とする。
- 原子的なhead制約を広告するには、チェックと保存だけのDB transactionでは不足する。
  Git receive/PR同期と協調した排他またはrevision fencingを実装し、その境界の競合テストを必須とする。
  それを実装できなければ、その能力をfalseとして厳密要求を拒否する。
- idempotency keyを実装する場合は主体・対象・要求digestと保存IDを一意に結び付ける。
  同一keyの同一要求は同じ保存ID、異なる要求は409とする。通知/Webhookの一回配送まで保証しない。
  成功応答は保存IDと読み返した原文・位置・PR関連を返す。応答喪失は照合できるようにする。

## 残作業と受入条件

| 単位 | 受入条件 |
| --- | --- |
| P09 | 通常/commit/file/line、同一IDの別保存元、両DOM、折りたたみ、日本語・特殊パス、旧/新行、改名・削除、GUEST、head追加/置換、マージ後、構造破損を検証。保存ID集合と取得範囲の差を正しく表し、HTMLを原文にしない |
| P10 | 現行差分と位置の検証、古いhead・PR外commit・無効path/行・保持不能本文の送信前拒否、保存後切断・不明結果・事後照合、重複自動投稿なしを検証。競合窓と非対応条件を公開 |
| 拡張経路 | 読取り権限で他者原文と明示PR関連の全履歴、認可境界、他PR混入なし、原文保持、原子的制約の実証、cursor整合性を検証してから完全性能力を有効化 |

本調査で実験したのは上記2版・ルートパス・標準イメージ・ローカル認証である。
コンテキストパス、外部SSO、独自プラグイン、fork由来PR、別PR共有commit、巨大/binary差分、
削除済みアカウント、Git objectのGC後は未検証。P09/P10で対象に加えるか、非対応条件として
明示する。調査完了と機能の利用開始は別に判定する。
