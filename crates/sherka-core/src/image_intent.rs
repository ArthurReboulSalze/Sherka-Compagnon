//! Explicit image-generator requests in the user's own words, in the seven
//! interface languages. Like the web and memory intents this is a local,
//! phrase-based decision: the application acts, the model only talks about it.
//! Phrases are folded (lower case, no accents, no hyphens or punctuation, only
//! `|` between clauses) and must start and end on a word boundary, so that a
//! disable is never read as an enable hidden inside it.
use std::sync::LazyLock;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ImageIntent {
    /// Switch the generator on: the interface switch, nothing drawn.
    SwitchOn,
    SwitchOff,
    /// A request for a picture: also wins over a switch-on in the same message,
    /// the application switches the generator on first.
    Make,
}

struct Grammar {
    make: Vec<String>,
    on: Vec<String>,
    off: Vec<String>,
}

/// What the user asked of the image generator, if they asked explicitly.
/// Negated requests and lone words such as image or imagine give `None`.
pub fn image_intent(text: &str) -> Option<ImageIntent> {
    let s = fold(&bounded(text));
    let grammar = &*GRAMMAR;
    if last_match(&s, &grammar.make).is_some() {
        return Some(ImageIntent::Make);
    }
    match (last_match(&s, &grammar.on), last_match(&s, &grammar.off)) {
        (Some(on), Some(off)) => Some(if on > off {
            ImageIntent::SwitchOn
        } else {
            ImageIntent::SwitchOff
        }),
        (Some(_), None) => Some(ImageIntent::SwitchOn),
        (None, Some(_)) => Some(ImageIntent::SwitchOff),
        (None, None) => None,
    }
}

/// Whether the message also asks to switch the generator on ("switch it on and
/// make me an image of…"): a request does not mention it otherwise. Negated and
/// later-cancelled switches do not count.
pub fn asks_switch_on(text: &str) -> bool {
    let s = fold(&bounded(text));
    let grammar = &*GRAMMAR;
    match (last_match(&s, &grammar.on), last_match(&s, &grammar.off)) {
        (Some(on), Some(off)) => on > off,
        (Some(_), None) => true,
        _ => false,
    }
}

/// A request is short; a pasted document is not scanned end to end.
fn bounded(text: &str) -> String {
    let count = text.chars().count();
    if count <= 2000 {
        return text.into();
    }
    let head: String = text.chars().take(1000).collect();
    let tail: String = text.chars().skip(count - 1000).collect();
    format!("{head}, {tail}")
}

fn fold(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for c in text.chars().flat_map(char::to_lowercase) {
        match c {
            '’' | '‘' | 'ʼ' | '`' | '´' => out.push('\''),
            'à' | 'á' | 'â' | 'ä' | 'ã' | 'å' => out.push('a'),
            'ç' => out.push('c'),
            'é' | 'è' | 'ê' | 'ë' => out.push('e'),
            'í' | 'ì' | 'î' | 'ï' => out.push('i'),
            'ó' | 'ò' | 'ô' | 'ö' | 'õ' => out.push('o'),
            'ú' | 'ù' | 'û' | 'ü' => out.push('u'),
            'ñ' => out.push('n'),
            'ý' | 'ÿ' => out.push('y'),
            'œ' => out.push_str("oe"),
            'æ' => out.push_str("ae"),
            'ß' => out.push_str("ss"),
            '\'' => out.push(c),
            '-' | '‐' | '‑' | '–' | '—' | '_' => out.push(' '),
            c if c.is_alphanumeric() => out.push(c),
            c if c.is_whitespace() => out.push(' '),
            _ => out.push_str(" | "),
        }
    }
    out.split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .replace("' ", "'")
}

// i18n: data-start
/// Words that cancel a request when one of the three words before it says so.
const NEGATORS: &[&str] = &[
    "ne", "pas", "jamais", "sans", "arrete", "arreter", "arretez", "cesse", "stop", "stoppe", "no",
    "not", "never", "don't", "dont", "doesn't", "won't", "wouldn't", "without", "nunca", "deja",
    "dejes", "sin", "non", "mai", "senza", "smetti", "basta", "nicht", "nie", "niemals", "kein",
    "keine", "keinen", "ohne",
];
/// Chinese and Japanese have no spaces: the few characters before are checked.
const CJK_NEGATORS: &[&str] = &[
    "不", "别", "別", "勿", "停", "取消", "关", "關", "无需", "無需",
];
/// A Chinese phrase followed by one of these is another word ("图表" chart).
const CJK_NOT_AFTER: &[&str] = &[
    "表", "书", "形", "纸", "层", "标", "案", "功能", "软件", "工具",
];

fn negated(before: &str) -> bool {
    let clause = before.rsplit('|').next().unwrap_or_default();
    clause
        .split_whitespace()
        .rev()
        .take(3)
        .any(|t| NEGATORS.contains(&t) || t.starts_with("n'"))
        || {
            let tail: String = {
                let mut chars: Vec<char> = clause.chars().rev().take(3).collect();
                chars.reverse();
                chars.into_iter().collect()
            };
            CJK_NEGATORS.iter().any(|n| tail.contains(n))
        }
}
// i18n: data-end

fn valid_at(s: &str, at: usize, phrase: &str) -> bool {
    let (before, after) = (&s[..at], &s[at + phrase.len()..]);
    if phrase.starts_with(|c: char| c.is_ascii_alphabetic())
        && before
            .chars()
            .next_back()
            .is_some_and(char::is_alphanumeric)
    {
        return false;
    }
    if phrase.ends_with(|c: char| c.is_ascii_alphabetic())
        && after.chars().next().is_some_and(char::is_alphanumeric)
    {
        return false;
    }
    if phrase.ends_with(|c: char| !c.is_ascii() && c.is_alphabetic())
        && CJK_NOT_AFTER.iter().any(|w| after.starts_with(w))
    {
        return false;
    }
    !negated(before)
}

/// Byte position of the last valid occurrence of any phrase.
fn last_match(s: &str, phrases: &[String]) -> Option<usize> {
    phrases
        .iter()
        .flat_map(|p| s.match_indices(p.as_str()).map(move |(at, _)| (at, p)))
        .filter(|(at, p)| valid_at(s, *at, p))
        .map(|(at, _)| at)
        .max()
}

/// Every concatenation of one entry from each list, folded.
fn cross(parts: &[&[&str]]) -> Vec<String> {
    let mut all = vec![String::new()];
    for part in parts {
        all = all
            .iter()
            .flat_map(|head| part.iter().map(move |tail| format!("{head}{tail}")))
            .collect();
    }
    all.iter().map(|p| fold(p)).collect()
}

fn phrases(items: &[&str]) -> Vec<String> {
    items.iter().map(|p| fold(p)).collect()
}

static GRAMMAR: LazyLock<Grammar> = LazyLock::new(|| {
    let mut grammar = Grammar {
        make: vec![],
        on: vec![],
        off: vec![],
    };
    for language in [french, english, spanish, italian, german, chinese, japanese] {
        let (make, on, off) = language();
        grammar.make.extend(make);
        grammar.on.extend(on);
        grammar.off.extend(off);
    }
    for list in [&mut grammar.make, &mut grammar.on, &mut grammar.off] {
        list.retain(|p| !p.is_empty());
        list.sort();
        list.dedup();
    }
    grammar
});

type Language = (Vec<String>, Vec<String>, Vec<String>);

// i18n: data-start
#[rustfmt::skip]
fn french() -> Language {
    let one = [
        "une image", "un dessin", "une illustration", "un portrait", "une peinture", "une photo",
        "un selfie", "une autre image", "une nouvelle image", "un autre dessin",
        "un nouveau dessin", "encore une image", "encore un dessin",
    ];
    let many = [
        "des images", "des dessins", "des illustrations", "quelques images", "plusieurs images",
    ];
    let with_me = [
        "fais moi ", "fais nous ", "genere moi ", "genere nous ", "cree moi ", "cree nous ",
        "dessine moi ", "dessine nous ", "produis moi ", "realise moi ", "fabrique moi ",
        "montre moi ", "montre nous ", "envoie moi ", "donne moi ", "illustre moi ", "refais moi ",
        "regenere moi ", "fais moi voir ", "fais nous voir ", "laisse moi voir ", "je veux ",
        "je voudrais ", "j'aimerais ", "j'aimerais voir ", "je veux voir ", "je voudrais voir ",
        "j'aimerais bien ", "je veux bien ",
    ];
    let bare = [
        "fais ", "genere ", "cree ", "dessine ", "produis ", "realise ", "fabrique ", "montre ",
        "refais ", "regenere ", "que tu me fasses ", "que tu me generes ", "que tu me dessines ",
        "que tu me crees ", "que tu fasses ", "que tu generes ", "que tu dessines ",
        "que tu crees ",
    ];
    let polite = [
        "peux tu ", "peux tu me ", "tu peux ", "tu peux me ", "pourrais tu ", "pourrais tu me ",
        "tu pourrais ", "tu pourrais me ", "veux tu ", "veux tu me ", "tu veux bien ",
        "tu veux bien me ", "voudrais tu ", "voudrais tu me ", "me ", "nous ",
    ];
    let infinitives = [
        "faire ", "generer ", "creer ", "dessiner ", "produire ", "realiser ", "fabriquer ",
        "montrer ",
    ];
    let mut make = cross(&[&with_me, &one]);
    make.extend(cross(&[&with_me, &many]));
    make.extend(cross(&[&bare, &one]));
    make.extend(cross(&[&polite, &infinitives, &one]));
    make.extend(phrases(&["dessine moi", "dessine nous", "dessine un", "dessine une"]));
    let things = [
        "le generateur", "ton generateur", "mon generateur", "les images", "les illustrations",
        "la generation d'image", "la generation d'illustration", "le moteur d'image",
    ];
    let on = [
        "allume ", "active ", "lance ", "demarre ", "remets ", "rallume ", "reactive ", "relance ",
    ];
    let on_inf = [
        "allumer ", "activer ", "lancer ", "demarrer ", "rallumer ", "reactiver ", "relancer ",
    ];
    let mut turn_on = cross(&[&on, &things]);
    turn_on.extend(cross(&[&on_inf, &things]));
    turn_on.extend(cross(&[&["mets en marche ", "remets en marche "], &things]));
    turn_on.extend(cross(&[
        &["mets ", "remets "],
        &["le generateur", "le generateur d'image", "le generateur d'images", "les images", "la generation d'images"],
        &[" en marche", " en route"],
    ]));
    let off = [
        "eteins ", "desactive ", "coupe ", "arrete ", "stoppe ", "stop ", "suspends ",
        "mets en pause ",
    ];
    let off_inf = ["eteindre ", "desactiver ", "couper ", "arreter ", "stopper ", "suspendre "];
    let mut turn_off = cross(&[&off, &things]);
    turn_off.extend(cross(&[&off_inf, &things]));
    turn_off.extend(phrases(&[
        "arrete de generer des images", "arrete de faire des images", "arrete de creer des images",
        "arrete de me faire des images", "arrete de dessiner",
        "arrete de generer des illustrations", "arrete de faire des illustrations",
        "n'utilise plus le generateur", "plus besoin du generateur",
    ]));
    (make, turn_on, turn_off)
}

#[rustfmt::skip]
fn english() -> Language {
    let one = [
        "an image", "a picture", "a drawing", "an illustration", "a portrait", "a painting",
        "a photo", "a selfie", "a sketch", "a pic", "another image", "another picture",
        "a new image", "a new picture", "another drawing", "one more image", "one more picture",
    ];
    let many = ["images", "pictures", "drawings", "illustrations", "photos"];
    let with_me = [
        "make me ", "generate me ", "create me ", "draw me ", "paint me ", "sketch me ", "show me ",
        "give me ", "send me ", "produce me ", "render me ", "get me ", "i want ", "i'd like ",
        "i would like ", "i want to see ", "i'd like to see ", "i would like to see ",
        "let me see ", "can i see ", "let's make ", "let's create ", "let's generate ",
        "let's draw ", "lets make ", "lets create ",
    ];
    let bare = [
        "make ", "generate ", "create ", "draw ", "paint ", "sketch ", "produce ", "render ",
        "illustrate ",
    ];
    let mut make = cross(&[&with_me, &one]);
    make.extend(cross(&[&with_me, &many]));
    make.extend(cross(&[&bare, &one]));
    make.extend(phrases(&["draw me", "paint me"]));
    let things = [
        "the image generator", "your image generator", "image generator", "the image generation",
        "image generation", "the images", "your images", "images", "the illustrations",
        "illustrations", "the picture generator", "the image maker",
    ];
    let on = [
        "turn on ", "switch on ", "enable ", "activate ", "start ", "start up ", "launch ",
        "power on ", "fire up ", "turn back on ", "switch back on ", "re enable ", "reenable ",
        "reactivate ", "restart ",
    ];
    let mut turn_on = cross(&[&on, &things]);
    turn_on.extend(cross(&[
        &["turn ", "switch "],
        &things,
        &[" on", " back on"],
    ]));
    let off = [
        "turn off ", "switch off ", "disable ", "deactivate ", "stop ", "shut off ", "shut down ",
        "kill ", "power off ", "pause ", "turn back off ",
    ];
    let mut turn_off = cross(&[&off, &things]);
    turn_off.extend(cross(&[&["turn ", "switch "], &things, &[" off"]]));
    turn_off.extend(phrases(&[
        "stop making images", "stop generating images", "stop creating images",
        "stop making pictures", "stop generating pictures", "stop drawing", "quit making images",
        "quit generating images", "no more images", "no more pictures", "no more illustrations",
        "no more drawings",
    ]));
    (make, turn_on, turn_off)
}

#[rustfmt::skip]
fn spanish() -> Language {
    let one = [
        "una imagen", "un dibujo", "una ilustracion", "una foto", "una fotografia", "un retrato",
        "una pintura", "un selfie", "otra imagen", "una nueva imagen", "una imagen nueva",
        "otro dibujo", "un nuevo dibujo",
    ];
    let many = ["imagenes", "dibujos", "ilustraciones", "fotos"];
    let with_me = [
        "hazme ", "haznos ", "dame ", "generame ", "creame ", "dibujame ", "dibujanos ", "pintame ",
        "muestrame ", "ensename ", "mandame ", "enviame ", "produceme ", "ilustrame ", "quiero ",
        "quiero ver ", "quisiera ", "quisiera ver ", "me gustaria ", "me gustaria ver ",
        "necesito ", "que me hagas ", "que me dibujes ", "que me generes ", "que me crees ",
    ];
    let bare = [
        "haz ", "genera ", "crea ", "dibuja ", "pinta ", "muestra ", "produce ", "ilustra ",
        "rehaz ",
    ];
    let polite = ["puedes ", "podrias ", "quieres "];
    let infinitives = [
        "hacerme ", "hacer ", "generarme ", "generar ", "crearme ", "crear ", "dibujarme ",
        "dibujar ", "pintarme ", "pintar ", "mostrarme ", "ensenarme ",
    ];
    let mut make = cross(&[&with_me, &one]);
    make.extend(cross(&[&with_me, &many]));
    make.extend(cross(&[&bare, &one]));
    make.extend(cross(&[&polite, &infinitives, &one]));
    make.extend(phrases(&["dibujame", "dibujame un", "dibujame una", "pintame un", "pintame una"]));
    let things = [
        "el generador", "tu generador", "las imagenes", "las ilustraciones",
        "la generacion de imagenes",
    ];
    let on = [
        "enciende ", "activa ", "prende ", "inicia ", "arranca ", "reactiva ", "pon en marcha ",
        "pon a funcionar ", "lanza ", "vuelve a activar ", "vuelve a encender ",
    ];
    let on_inf = [
        "encender ", "activar ", "iniciar ", "arrancar ", "prender ", "reactivar ", "lanzar ",
    ];
    let mut turn_on = cross(&[&on, &things]);
    turn_on.extend(cross(&[&on_inf, &things]));
    let off = ["apaga ", "desactiva ", "corta ", "deten ", "suspende ", "cierra "];
    let off_inf = ["apagar ", "desactivar ", "cortar ", "detener ", "parar ", "suspender "];
    let mut turn_off = cross(&[&off, &things]);
    turn_off.extend(cross(&[&off_inf, &things]));
    turn_off.extend(phrases(&[
        "deja de generar imagenes", "deja de hacer imagenes", "deja de crear imagenes",
        "deja de dibujar", "deja de generar ilustraciones", "ya no quiero imagenes",
        "no mas imagenes", "sin mas imagenes",
    ]));
    (make, turn_on, turn_off)
}

#[rustfmt::skip]
fn italian() -> Language {
    let one = [
        "un'immagine", "un immagine", "un disegno", "un'illustrazione", "una foto",
        "una fotografia", "un ritratto", "un dipinto", "un selfie", "un'altra immagine",
        "una nuova immagine", "un altro disegno", "un nuovo disegno",
    ];
    let many = ["immagini", "disegni", "illustrazioni", "foto"];
    let with_me = [
        "fammi ", "facci ", "dammi ", "generami ", "creami ", "disegnami ", "disegnaci ",
        "dipingimi ", "mostrami ", "fammi vedere ", "mandami ", "inviami ", "illustrami ",
        "voglio ", "voglio vedere ", "vorrei ", "vorrei vedere ", "mi piacerebbe ",
        "mi piacerebbe vedere ", "mi fai ", "mi crei ", "mi disegni ", "mi generi ", "mi mostri ",
        "mi dai ", "mi mandi ",
    ];
    let bare = [
        "fai ", "genera ", "crea ", "disegna ", "dipingi ", "mostra ", "produci ", "illustra ",
    ];
    let polite = ["puoi ", "potresti ", "vuoi "];
    let infinitives = [
        "fare ", "farmi ", "generare ", "generarmi ", "creare ", "crearmi ", "disegnare ",
        "disegnarmi ", "dipingere ", "mostrare ", "mostrarmi ",
    ];
    let mut make = cross(&[&with_me, &one]);
    make.extend(cross(&[&with_me, &many]));
    make.extend(cross(&[&bare, &one]));
    make.extend(cross(&[&polite, &infinitives, &one]));
    make.extend(phrases(&["disegnami", "disegna un", "disegna una"]));
    let things = [
        "il generatore", "il tuo generatore", "le immagini", "le illustrazioni",
        "la generazione di immagini",
    ];
    let on = [
        "accendi ", "attiva ", "avvia ", "lancia ", "riattiva ", "riaccendi ", "metti in funzione ",
        "metti in moto ",
    ];
    let on_inf = [
        "accendere ", "attivare ", "avviare ", "lanciare ", "riattivare ", "riaccendere ",
    ];
    let mut turn_on = cross(&[&on, &things]);
    turn_on.extend(cross(&[&on_inf, &things]));
    let off = ["spegni ", "disattiva ", "ferma ", "interrompi ", "stoppa ", "chiudi ", "sospendi "];
    let off_inf = ["spegnere ", "disattivare ", "fermare ", "interrompere ", "sospendere "];
    let mut turn_off = cross(&[&off, &things]);
    turn_off.extend(cross(&[&off_inf, &things]));
    turn_off.extend(phrases(&[
        "smetti di generare immagini", "smetti di fare immagini", "smetti di creare immagini",
        "smetti di disegnare", "basta con le immagini", "basta immagini", "niente piu immagini",
        "non generare piu immagini",
    ]));
    (make, turn_on, turn_off)
}

#[rustfmt::skip]
fn german() -> Language {
    let one = [
        "ein bild", "ein foto", "eine zeichnung", "eine illustration", "ein portrait",
        "ein portrat", "ein gemalde", "ein selfie", "noch ein bild", "ein neues bild",
        "ein anderes bild", "eine neue zeichnung",
    ];
    let many = ["bilder", "zeichnungen", "illustrationen", "fotos"];
    let with_me = [
        "mach mir ", "mache mir ", "erstelle mir ", "erstell mir ", "generiere mir ",
        "erzeuge mir ", "zeichne mir ", "male mir ", "mal mir ", "zeig mir ", "zeige mir ",
        "gib mir ", "schick mir ", "schicke mir ", "sende mir ", "kannst du mir ",
        "konntest du mir ", "ich will ", "ich mochte ", "ich mochte gern ", "ich mochte gerne ",
        "ich hatte gern ", "ich hatte gerne ", "ich wurde gern ", "ich wurde gerne ", "lass uns ",
    ];
    let bare = [
        "mach ", "mache ", "erstelle ", "generiere ", "erzeuge ", "zeichne ", "male ", "mal ",
    ];
    let verbs = [
        " machen", " erstellen", " generieren", " erzeugen", " zeichnen", " malen", " zeigen",
        " schicken", " senden",
    ];
    let mut make = cross(&[&with_me, &one]);
    make.extend(cross(&[&with_me, &many]));
    make.extend(cross(&[&bare, &one]));
    make.extend(cross(&[&one, &verbs]));
    let things = [
        "den bildgenerator", "deinen bildgenerator", "den illustrationsgenerator",
        "die bildgenerierung", "die bilderzeugung", "die bilder", "die illustrationen",
        "den generator",
    ];
    let switch = ["schalte ", "schalte bitte ", "schalt ", "mach ", "mach bitte ", "mache "];
    let mut turn_on = cross(&[
        &switch,
        &things,
        &[" ein", " an", " wieder ein", " wieder an"],
    ]);
    turn_on.extend(cross(&[
        &["aktiviere ", "aktivier ", "reaktiviere ", "starte ", "starte wieder "],
        &things,
    ]));
    turn_on.extend(cross(&[
        &things,
        &[" einschalten", " anschalten", " aktivieren", " starten", " anmachen"],
    ]));
    let mut turn_off = cross(&[&switch, &things, &[" aus", " ab", " wieder aus"]]);
    turn_off.extend(cross(&[
        &["deaktiviere ", "deaktivier ", "stoppe ", "stopp ", "beende "],
        &things,
    ]));
    turn_off.extend(cross(&[
        &things,
        &[
            " ausschalten", " abschalten", " deaktivieren", " stoppen", " beenden", " ausmachen",
            " abstellen",
        ],
    ]));
    turn_off.extend(phrases(&[
        "hor auf bilder zu", "hor auf, bilder zu", "hor auf mit den bildern", "keine bilder mehr",
    ]));
    (make, turn_on, turn_off)
}

#[rustfmt::skip]
fn chinese() -> Language {
    let leads = [
        "画", "给我画", "帮我画", "为我画", "替我画", "请画", "请帮我画", "给我生成", "帮我生成", "生成", "创作", "制作", "给我做",
        "帮我做", "做", "来", "发", "出", "给我来", "给我", "给我看", "给我看看", "给我发", "我想要", "我要", "我想看", "想看",
        "绘制", "帮我绘制", "给我绘制", "再来", "再画", "再给我画", "再生成", "再做",
    ];
    let counters = ["一张", "一幅", "一个", "一副", "张", "幅", "个", "几张", "几幅"];
    let nouns = ["图", "图片", "图像", "插画", "照片", "画", "肖像", "自拍", "插图", "漫画"];
    let bare_nouns = ["图", "图片", "图像", "插画", "照片", "插图"];
    let mut make = cross(&[&leads, &counters, &nouns]);
    make.extend(cross(&[&leads, &[""], &bare_nouns]));
    make.extend(phrases(&[
        "给我画", "帮我画", "为我画", "替我画", "请画", "请帮我画", "画一张", "画一幅", "画一个", "画个", "生成一张", "生成一幅",
    ]));
    let things = [
        "图片生成器", "图像生成器", "图片生成", "图像生成", "生成器", "插画生成", "画图功能", "绘图功能", "配图", "图片功能", "图像功能",
        "绘画功能", "作图功能",
    ];
    let turn_on = cross(&[
        &["打开", "开启", "启动", "启用", "开", "激活", "重新打开", "重新开启", "重启"],
        &things,
    ]);
    let mut turn_off = cross(&[
        &["关闭", "关掉", "关", "停止", "停用", "禁用", "取消", "暂停", "关上"],
        &things,
    ]);
    turn_off.extend(phrases(&[
        "别再生成图片", "别再生成图像", "别再画图", "别再配图", "不要再生成图片", "不要再画图", "不要再配图", "不用再生成图片", "不用再画图",
        "不用再配图", "停止画图", "停止生成图片", "停止生成图像", "别再给我画图", "不要再给我画图", "别再画了", "不要再画了",
    ]));
    (make, turn_on, turn_off)
}

#[rustfmt::skip]
fn japanese() -> Language {
    let nouns = ["画像", "イラスト", "絵", "写真", "ポートレート", "自撮り", "スケッチ", "絵画", "似顔絵", "挿絵"];
    let verbs = [
        "を作って", "を作成して", "を生成して", "を描いて", "を出して", "を見せて", "をください", "を送って", "を書いて", "が欲しい", "がほしい",
        "が見たい", "を見たい", "をお願い", "を1枚", "を一枚", "を作ろう", "を描こう",
    ];
    let mut make = cross(&[&nouns, &verbs]);
    make.extend(phrases(&[
        "を描いて", "描いてください", "描いてほしい", "描いて欲しい", "描いてくれる", "描いてくれない", "描いてもらえる", "描いてもらえない", "画像生成して",
        "イラスト生成して", "お絵描きして", "お絵かきして",
    ]));
    let strict = [
        "画像生成", "画像生成機能", "画像ジェネレーター", "画像ジェネレータ", "ジェネレーター", "イラスト生成", "画像生成器", "画像機能", "イラスト機能",
    ];
    let loose = ["画像", "イラスト"];
    let mut turn_on = cross(&[
        &strict,
        &["をオンに", "をつけて", "を起動して", "を有効に", "を有効化して", "を立ち上げて", "を再開して"],
    ]);
    turn_on.extend(cross(&[&loose, &["をオンに", "を有効に", "を起動して"]]));
    let mut turn_off = cross(&[
        &strict,
        &["をオフに", "を止めて", "を停止して", "を無効に", "を切って", "をやめて", "を終了して", "をストップして", "を無効化して"],
    ]);
    turn_off.extend(cross(&[&loose, &["をオフに", "を無効に", "を停止して"]]));
    turn_off.extend(phrases(&[
        "画像を作るのをやめて", "絵を描くのをやめて", "画像を生成するのをやめて", "イラストを描くのをやめて", "もう画像はいらない", "もう画像はいりません",
        "画像はもういらない",
    ]));
    (make, turn_on, turn_off)
}
// i18n: data-end

#[cfg(test)]
mod tests {
    use super::*;
    use ImageIntent::{Make, SwitchOff, SwitchOn};

    fn check(cases: &[(&str, Option<ImageIntent>)]) {
        for (text, expected) in cases {
            assert_eq!(image_intent(text), *expected, "{text}");
        }
    }

    #[test]
    fn french_requests_and_switches() {
        check(&[
            ("Fais-moi une image d'un chat astronaute", Some(Make)),
            ("fais une image de la mer", Some(Make)),
            ("Génère-moi une illustration de la forêt", Some(Make)),
            ("crée une image de nous deux", Some(Make)),
            ("Dessine-moi un mouton", Some(Make)),
            ("dessine moi", Some(Make)),
            ("Montre-moi une image de la plage", Some(Make)),
            ("Peux-tu me faire un dessin de ton bureau ?", Some(Make)),
            ("Je voudrais une photo de toi en cuisine", Some(Make)),
            ("fais moi des images de chats", Some(Make)),
            ("génère une image de dragon", Some(Make)),
            ("Tu peux me générer une image de la mer ?", Some(Make)),
            ("dessine un chat", Some(Make)),
            ("fais-moi un selfie", Some(Make)),
            ("je voudrais voir une illustration de ton monde", Some(Make)),
            ("allume les images", Some(SwitchOn)),
            ("Active le générateur d'image", Some(SwitchOn)),
            ("mets le générateur d'images en marche", Some(SwitchOn)),
            ("éteins les images", Some(SwitchOff)),
            ("arrête de me faire des images", Some(SwitchOff)),
            ("Allume le générateur d'images", Some(SwitchOn)),
            ("active le générateur", Some(SwitchOn)),
            (
                "Lance le générateur d'images, s'il te plaît",
                Some(SwitchOn),
            ),
            ("démarre le générateur", Some(SwitchOn)),
            ("active les images", Some(SwitchOn)),
            ("tu peux allumer le générateur ?", Some(SwitchOn)),
            ("Éteins le générateur", Some(SwitchOff)),
            ("désactive le générateur d'images", Some(SwitchOff)),
            ("Coupe le générateur d'images.", Some(SwitchOff)),
            ("arrête le générateur", Some(SwitchOff)),
            ("arrête les images", Some(SwitchOff)),
            ("coupe les images", Some(SwitchOff)),
            ("désactive les images", Some(SwitchOff)),
            ("arrête de faire des images", Some(SwitchOff)),
        ]);
    }

    #[test]
    fn make_wins_over_a_switch_in_the_same_message() {
        check(&[
            (
                "Allume le générateur et fais-moi une image de la mer",
                Some(Make),
            ),
            (
                "switch on the image generator and make me a picture of a fox",
                Some(Make),
            ),
            (
                "Allume le générateur, puis dessine-moi un dragon",
                Some(Make),
            ),
        ]);
    }

    #[test]
    fn the_last_switch_wins_when_both_are_said() {
        check(&[
            (
                "éteins le générateur... non, allume le générateur",
                Some(SwitchOn),
            ),
            (
                "allume le générateur, finalement éteins le générateur",
                Some(SwitchOff),
            ),
        ]);
    }

    #[test]
    fn english_requests_and_switches() {
        check(&[
            ("Make me an image of a cat astronaut", Some(Make)),
            ("make a picture of the sea", Some(Make)),
            ("can you draw me a dragon?", Some(Make)),
            ("Generate an illustration of a forest", Some(Make)),
            ("show me a picture of the beach", Some(Make)),
            ("I'd like an image of us at the lake", Some(Make)),
            ("create a selfie of you", Some(Make)),
            ("Turn on the image generator", Some(SwitchOn)),
            ("switch on the image generator please", Some(SwitchOn)),
            ("enable images", Some(SwitchOn)),
            ("turn the image generator on", Some(SwitchOn)),
            ("Turn off the image generator", Some(SwitchOff)),
            ("switch the image generator off", Some(SwitchOff)),
            ("disable the images", Some(SwitchOff)),
            ("stop the images", Some(SwitchOff)),
            ("stop generating images", Some(SwitchOff)),
        ]);
    }

    #[test]
    fn spanish_requests_and_switches() {
        check(&[
            ("Hazme una imagen de un gato astronauta", Some(Make)),
            ("dibújame un dragón", Some(Make)),
            ("genera una ilustración del bosque", Some(Make)),
            ("muéstrame una imagen de la playa", Some(Make)),
            ("¿Puedes hacer un dibujo de tu oficina?", Some(Make)),
            ("Enciende el generador de imágenes", Some(SwitchOn)),
            ("activa las imágenes", Some(SwitchOn)),
            ("Apaga el generador de imágenes", Some(SwitchOff)),
            ("desactiva el generador", Some(SwitchOff)),
            ("deja de generar imágenes", Some(SwitchOff)),
        ]);
    }

    #[test]
    fn italian_requests_and_switches() {
        check(&[
            ("Fammi un'immagine di un gatto astronauta", Some(Make)),
            ("disegnami un drago", Some(Make)),
            ("genera un'illustrazione della foresta", Some(Make)),
            ("mostrami un'immagine della spiaggia", Some(Make)),
            ("Mi fai un disegno del tuo ufficio?", Some(Make)),
            ("Accendi il generatore di immagini", Some(SwitchOn)),
            ("attiva le immagini", Some(SwitchOn)),
            ("Spegni il generatore di immagini", Some(SwitchOff)),
            ("disattiva il generatore", Some(SwitchOff)),
            ("smetti di generare immagini", Some(SwitchOff)),
        ]);
    }

    #[test]
    fn german_requests_and_switches() {
        check(&[
            ("Mach mir ein Bild von einer Katze", Some(Make)),
            ("Zeichne mir ein Bild von einem Drachen", Some(Make)),
            ("Erstelle mir eine Illustration vom Wald", Some(Make)),
            ("Kannst du mir ein Bild vom Strand machen?", Some(Make)),
            ("Zeig mir ein Bild vom See", Some(Make)),
            ("Schalte den Bildgenerator ein", Some(SwitchOn)),
            ("schalte bitte den Bildgenerator wieder an", Some(SwitchOn)),
            ("Aktiviere die Bilder", Some(SwitchOn)),
            ("Schalte den Bildgenerator aus", Some(SwitchOff)),
            ("Deaktiviere den Bildgenerator", Some(SwitchOff)),
            ("Hör auf, Bilder zu machen", Some(SwitchOff)),
            ("Hör auf Bilder zu machen", Some(SwitchOff)),
        ]);
    }

    #[test]
    fn chinese_requests_and_switches() {
        check(&[
            ("给我画一张猫的图", Some(Make)),
            ("帮我生成一张森林的图片", Some(Make)),
            ("画一幅海边的画", Some(Make)),
            ("来张图", Some(Make)),
            ("我想看一张自拍", Some(Make)),
            ("打开图片生成器", Some(SwitchOn)),
            ("请开启图像生成器", Some(SwitchOn)),
            ("关闭图片生成器", Some(SwitchOff)),
            ("关掉图片生成", Some(SwitchOff)),
            ("别再生成图片了", Some(SwitchOff)),
            ("不要再画图了", Some(SwitchOff)),
        ]);
    }

    #[test]
    fn japanese_requests_and_switches() {
        check(&[
            ("猫の画像を作って", Some(Make)),
            ("森のイラストを描いて", Some(Make)),
            ("猫を描いてください", Some(Make)),
            ("海の絵を見せて", Some(Make)),
            ("画像が欲しい", Some(Make)),
            ("画像生成をオンにして", Some(SwitchOn)),
            ("画像ジェネレーターを起動して", Some(SwitchOn)),
            ("画像生成をオフにして", Some(SwitchOff)),
            ("画像生成を止めて", Some(SwitchOff)),
            ("絵を描くのをやめて", Some(SwitchOff)),
        ]);
    }

    #[test]
    fn negations_never_trigger() {
        check(&[
            ("Ne fais pas d'image", None),
            ("ne génère pas d'image", None),
            ("Surtout ne me dessine pas un chat", None),
            ("ne fais pas une image de ça", None),
            ("ne me fais pas une image", None),
            ("Je ne veux pas d'image pour l'instant", None),
            ("pas d'image s'il te plaît", None),
            ("n'allume pas le générateur", None),
            ("ne désactive pas le générateur", None),
            ("n'éteins pas le générateur", None),
            ("Don't make an image", None),
            ("please do not generate a picture of that", None),
            ("never draw me a cat", None),
            ("don't turn on the image generator", None),
            ("no hagas una imagen", None),
            ("No me hagas una imagen ahora", None),
            ("non farmi un'immagine", None),
            ("Mach mir kein Bild", None),
            ("nicht den Bildgenerator einschalten", None),
            ("不要画一张图", None),
            ("别给我画一张猫的图", None),
            ("别给我画一张图", None),
            ("画像を作らないで", None),
        ]);
    }

    #[test]
    fn a_disable_is_not_read_as_an_enable() {
        check(&[
            ("disable images", Some(SwitchOff)),
            ("deactivate the image generator", Some(SwitchOff)),
            ("désactive les images", Some(SwitchOff)),
            ("Deaktiviere die Bilder", Some(SwitchOff)),
        ]);
    }

    #[test]
    fn ordinary_talk_about_pictures_is_not_a_request() {
        check(&[
            ("image", None),
            ("Image", None),
            ("picture", None),
            ("imagine", None),
            ("Imagine que tu sois un pirate", None),
            ("J'ai vu une belle image hier", None),
            ("Cette image me rappelle mon enfance", None),
            ("remember this picture", None),
            ("I saw a picture of a cat yesterday", None),
            ("What a beautiful image!", None),
            ("Quel genre d'images tu fais ?", None),
            ("Tu fais des images ?", None),
            ("Do you make images?", None),
            ("Tu peux allumer la lumière ?", None),
            ("Le générateur de courant est en panne", None),
            ("Mon image de profil est floue", None),
            ("Wie war dein Bild gestern?", None),
            ("今日は画像の話をしよう", None),
            ("这张图表很清楚", None),
            ("给我一个图书馆的地址", None),
            ("", None),
        ]);
    }

    #[test]
    fn a_request_may_also_ask_to_switch_the_generator_on() {
        for text in [
            "Allume le générateur et fais-moi une image de la mer",
            "Allume le générateur, puis dessine-moi un dragon",
            "switch on the image generator and make me a picture of a fox",
            "Enciende el generador de imágenes y hazme una imagen de un gato",
            "Accendi il generatore di immagini e fammi un'immagine di un drago",
            "Schalte den Bildgenerator ein und mach mir ein Bild von einer Katze",
            "打开图片生成器，给我画一张猫的图",
            "画像生成をオンにして、猫の画像を作って",
            "Allume le générateur",
        ] {
            assert!(asks_switch_on(text), "{text}");
        }
        for text in [
            "Fais-moi une image de la mer",
            "Make me a picture of a fox",
            "N'allume pas le générateur mais fais-moi une image de la mer",
            "don't turn on the image generator, just make me a picture",
            "allume le générateur... non, éteins le générateur et fais-moi une image",
            "Éteins le générateur",
            "Allume la lumière et fais-moi une image",
            "",
        ] {
            assert!(!asks_switch_on(text), "{text}");
        }
    }

    #[test]
    fn a_decision_is_fast() {
        let start = std::time::Instant::now();
        for _ in 0..200 {
            image_intent("Peux-tu me raconter une histoire sur la lune, s'il te plaît ?");
        }
        assert!(start.elapsed() < std::time::Duration::from_secs(5));
        assert!(GRAMMAR.make.len() + GRAMMAR.on.len() + GRAMMAR.off.len() < 20_000);
    }

    #[test]
    fn a_long_message_is_still_read_at_both_ends() {
        let filler = "bla ".repeat(1500);
        assert_eq!(
            image_intent(&format!("fais-moi une image {filler}")),
            Some(Make)
        );
        assert_eq!(
            image_intent(&format!("{filler} éteins le générateur")),
            Some(SwitchOff)
        );
    }
}
