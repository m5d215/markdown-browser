# 文書ショーケース

これは **markdown-browser** の機能確認用 markdown。全部入りなので、これ
1 本通して崩れがないか目視確認する用途。日本語メインで書いているので、
マルチバイト・East Asian Width 関連のバグもこれで炙り出せる想定。

## 目次の確認

`o` キーで TOC オーバーレイが開く。この文書には複数階層の見出しを意図的
に入れてあるので、レベル別インデントとジャンプ精度が確認できる。

## インライン整形

このパラグラフには **強調 (Strong)** 、 *斜体 (Emph)* 、
~~取り消し線 (Strikethrough)~~ 、それから `インラインコード` が含まれる。

入れ子の組み合わせ:

- **太字の中に *斜体* を入れる**
- ~~取り消し線の中に `code` を入れる~~
- *斜体の中に **太字** を入れる*

行内に半角と全角が混ざる文章は描画位置がずれやすい。たとえば
「ASCII text と 日本語 text の混在 + 「カギ括弧」 + 『二重括弧』 +
── 各種ダッシュ ── や、 ¶ § † ‡ などの記号も入る」 のような行。

絵文字 (4 byte UTF-8 / 表示幅 2):

🎉 🍣 🌸 ✨ 🚀 💻 🦀 📚 🔥 🌍

GitHub-style shortcode (`e` で展開を ON/OFF):

:rocket: :sparkles: :tada: :+1: :heart: :crab: :sushi: :nope_not_a_real_one:

合字 / 結合文字: café (combining acute) と café (precomposed) は
見た目同じになるはず。

## リンクの確認

外部 URL は `open` で OS デフォルトに飛ぶ:

- [GitHub](https://github.com)
- [Anthropic](https://anthropic.com)
- [mailto テスト](mailto:noreply@example.com)

同文書内のアンカー (Back で戻れる):

- [テーブルセクションへ](#テーブル)
- [コードブロックへ](#コードブロック)
- [トップに戻る](#文書ショーケース)

相対ファイル参照:

- [プロジェクトの README へ](../README.md)
- [存在しないファイル (エラー確認用)](./does-not-exist.md)

## リスト

順序なし、ネスト 3 階層:

- 一階層目
  - 二階層目
    - 三階層目に **強調** と `code` を入れる
    - 三階層目その 2
  - 二階層目に戻る
- 一階層目に戻る
- 日本語の長い項目: 折り返しが必要になるような長さの文章を入れて、
  二行目以降が正しくインデントされているかを目視で確認する

順序あり:

1. 一つ目
2. 二つ目
   1. ネストした順序付きリスト
   2. 続き
3. 三つ目

タスクリスト (GFM 拡張):

- [x] 完了したタスク
- [x] 別の完了タスク
- [ ] 未完のタスク
- [ ] 「日本語の未完タスク」

## 引用 (BlockQuote)

> ここは引用ブロック。左に `│` の縦罫線で示される。
>
> > ネストした引用。引用の中の引用も成立する。
> > 二行目もちゃんと縦罫線が付く。
>
> 引用内でも **強調** や `code` などのインライン整形は効く。

## テーブル

このプロジェクトの差別化要因として最初に作り込んだ部分。EAW
(East Asian Width)、ANSI を含むセル、alignment、絵文字、いずれも
崩さず整列することを目標にしている。

| 用途             | クレート       | 理由                                          |
|:-----------------|:--------------:|----------------------------------------------:|
| Markdown パース  | `comrak`       | GFM AST 対応、活発にメンテされている          |
| テーブル描画     | (自前)         | comfy-table も検討したが描画方針が合わず自作  |
| シンタックス強調 | `syntect`      | TextMate 文法、24-bit color、枯れている       |
| TUI              | `ratatui`      | 事実上のスタンダード                          |

混在チェック (左寄せ / 中央 / 右寄せ + 日本語・絵文字・ANSI):

| 左寄せ          | 中央         | 右寄せ |
|:----------------|:------------:|-------:|
| `inline_code`   | **太字**     |    100 |
| 日本語の長め文  | 🎉 emoji 🍣  |     42 |
| short           | 中央寄せ     |      7 |
| ¶ § † ‡         | a            | -1234  |

セル内にリンクを含むテーブル (Tab/Shift-Tab で選べる):

| 種別        | リンク                                        | 備考                        |
|:------------|:----------------------------------------------|:----------------------------|
| 外部 URL    | [GitHub](https://github.com)                  | OS デフォルトで開く         |
| 内部アンカー | [コードブロックへ](#コードブロック)           | 同 doc 内ジャンプ           |
| 相対パス    | [README へ](../README.md)                     | 別ファイルに切り替え        |
| エラー確認  | [存在しない](./missing.md)                    | status bar にエラー表示     |

## コードブロック

シンタックスハイライト。`syntect` 経由で各言語の文法を反映。背景色は
端末に任せるため、前景色と装飾のみ反映している。

### Rust

```rust
fn main() {
    // 日本語コメント: 整数の二乗を 0..10 で表示
    let xs: Vec<u32> = (0..10).map(|x| x * x).collect();
    println!("{xs:?}");
}

#[derive(Debug)]
struct ユーザー {
    name: String,
    age: u32,
}
```

### Python

```python
def fibonacci(n: int) -> list[int]:
    """フィボナッチ数列の最初の n 項を返す。"""
    a, b = 0, 1
    result = []
    for _ in range(n):
        result.append(a)
        a, b = b, a + b
    return result

print(fibonacci(10))
```

### TypeScript

```typescript
interface User {
    name: string;
    age: number;
}

function greet(user: User): string {
    return `こんにちは、${user.name} さん (${user.age} 歳)`;
}

const u: User = { name: "山田 太郎", age: 30 };
console.log(greet(u));
```

### Bash

```bash
#!/bin/bash
# シェルスクリプトのハイライト確認
for f in *.md; do
    echo "Processing: $f"
    wc -l "$f"
done
```

### JSON

```json
{
    "name": "markdown-browser",
    "version": "0.1.0",
    "features": ["table", "toc", "syntect"],
    "japanese": "日本語の値も入る",
    "nested": {
        "key": "value"
    }
}
```

### 言語指定なし

```
これは言語指定のないコードブロック。
syntect は plain text として扱うので、ハイライトは入らない。
ASCII 罫線も生のまま:
    +---+---+
    | a | b |
    +---+---+
```

## 水平線

下に thematic break が入る。

---

ここから下が thematic break の後の段落。

## 画像 (alt text のみ表示)

画像は MVP では実描画せず、alt と URL のみテキストで表示する:

![プレースホルダ画像](https://example.com/placeholder.png "タイトル")

これは Sixel / Kitty / iTerm2 inline image を将来差し込む口だけ
用意してある (`MediaRenderer` trait)。

## ネストの大きい例

引用の中にリストとリンクとコード:

> - 引用の中のリスト
>   - ネストした項目
>     - さらにネストして **強調**
> - 引用内に `inline code` も
> - 引用内のリンク [GitHub](https://github.com) も入る

リストの中に引用とコードブロック:

1. 番号付きリストの一項目
2. 二項目に引用を含める:

   > 引用の中身。
   > 二行目。

3. 三項目にコードブロックを含める:

   ```rust
   fn nested() -> &'static str {
       "ネストしたコードブロック"
   }
   ```

## エッジケース

- 長い URL: <https://example.com/very/long/path/that/should/wrap/if/the/terminal/is/narrow/enough.html>
- 連続する空白:    ここに  半角     スペースが多数
- 全角スペース:　　　全角スペース3個

## 終わり

ここまで全要素を通したつもり。気になる崩れがあれば追って詰める。
