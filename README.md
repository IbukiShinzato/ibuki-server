# ibuki-server

Cで実装されたファイルサーバをRustに書き換え、拡張したプロジェクト

## 実装した機能

- GET: ファイルの中身を取得
- PUT: ファイルをサーバ上に保存
- DEL: ファイルをサーバ上から削除
- LS: サーバ上にあるディレクトリエントリ名を取得

## 実行結果（見やすいように改行入れています）

```
> GET<test.txt>
NOT FOUND

> PUT<test.txt><test file>
PUT: test.txt saved

> GET<test.txt>
FILE(9): test file

> DEL<test.txt>
DEL: test.txt deleted

> GET<test.txt>
NOT FOUND

> LS
LIST(20): test1.txt
test2.txt
```
