// ODF-E2 事件流探針：把 content.xml 過一遍 quick_xml，印出 'Light vertical' 附近的
// 事件序列（含 GeneralRef），定位 quire-odf walker 為何把 svg:desc 內文收進段落。
use quick_xml::events::Event;
use quick_xml::Reader;

fn main() {
    let path = std::env::args().nth(1).expect("usage: evdump <xml-file> [needle]");
    let needle = std::env::args().nth(2).unwrap_or_else(|| "Light vertical".into());
    let xml = std::fs::read(&path).unwrap();
    let mut reader = Reader::from_reader(xml.as_slice());
    let mut buf = Vec::new();
    let mut stack: Vec<String> = Vec::new();
    let mut p_depth = 0usize;
    let mut log: Vec<String> = Vec::new();
    let mut window = 0usize;
    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Start(e)) => {
                let n = String::from_utf8_lossy(e.name().as_ref()).into_owned();
                if n == "text:p" || n == "text:h" {
                    p_depth += 1;
                }
                stack.push(n.clone());
                push(&mut log, &mut window, format!("Start <{n}> p_depth={p_depth}"));
            }
            Ok(Event::Empty(e)) => {
                let n = String::from_utf8_lossy(e.name().as_ref()).into_owned();
                push(&mut log, &mut window, format!("Empty <{n}> p_depth={p_depth}"));
            }
            Ok(Event::End(e)) => {
                let n = String::from_utf8_lossy(e.name().as_ref()).into_owned();
                if (n == "text:p" || n == "text:h") && p_depth > 0 {
                    p_depth -= 1;
                }
                if let Some(i) = stack.iter().rposition(|x| *x == n) {
                    stack.truncate(i);
                }
                push(&mut log, &mut window, format!("End </{n}> p_depth={p_depth}"));
            }
            Ok(Event::Text(e)) => {
                let t = String::from_utf8_lossy(e.as_ref()).into_owned();
                push(&mut log, &mut window, format!("Text {t:?} p_depth={p_depth}"));
            }
            Ok(Event::GeneralRef(e)) => {
                let t = String::from_utf8_lossy(e.as_ref()).into_owned();
                push(&mut log, &mut window, format!("GeneralRef &{t}; p_depth={p_depth}"));
            }
            Ok(Event::Eof) => break,
            Err(err) => {
                println!("ERR {err}");
                break;
            }
            _ => {}
        }
        buf.clear();
    }
    fn push(log: &mut Vec<String>, window: &mut usize, line: String) {
        log.push(line);
        if *window > 0 {
            *window -= 1;
        }
    }
    // 找 needle 所在 Text 事件，印前後 12 條
    let idx = log.iter().position(|l| l.contains(&needle)).unwrap_or(log.len());
    for (i, l) in log[idx.saturating_sub(12)..log.len().min(idx + 8)].iter().enumerate() {
        println!("{:>3} {}", idx.saturating_sub(12) + i, l);
    }
    // 另：印出首個 GeneralRef 前後
    if let Some(g) = log.iter().position(|l| l.starts_with("GeneralRef")) {
        println!("\n--- 首個 GeneralRef 附近 ---");
        for l in &log[g.saturating_sub(2)..log.len().min(g + 4)] {
            println!("    {l}");
        }
    } else {
        println!("\n（此檔無 GeneralRef 事件）");
    }
}
