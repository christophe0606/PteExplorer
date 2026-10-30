use super::*;

struct Layout<'a> {
    method: &'a Method,
    nodes: Vec<Node>,
    edges: Vec<Edge>,
    levels: Vec<usize>,
    next_y: BTreeMap<usize, f32>,
    values: BTreeMap<i32, usize>,
}
impl Layout<'_> {
    fn push(&mut self, mut node: Node, level: usize, desired_y: f32) -> usize {
        node.x = 32. + level as f32 * 408.;
        node.y = desired_y.max(*self.next_y.get(&level).unwrap_or(&48.));
        self.next_y.insert(level, node.y + node.h + 48.);
        let index = self.nodes.len();
        self.nodes.push(node);
        self.levels.push(level);
        index
    }
    fn value(&mut self, id: i32, level: usize, y: f32) -> usize {
        let m = self.method;
        let value = &m.values[id as usize];
        let label = match (m.inputs.contains(&id), m.outputs.contains(&id)) {
            (true, true) => "INPUT / OUTPUT",
            (true, _) => "INPUT",
            (_, true) => "OUTPUT",
            _ => "Tensor / value",
        };
        let index = self.push(
            Node {
                selection: Selection::Value(id as usize),
                x: 0.,
                y: 0.,
                w: 300.,
                h: 80.,
                title: format!("{label} · v{id}"),
                subtitle: value.description.clone(),
                properties: vec![],
                delegate: false,
            },
            level,
            y,
        );
        self.values.insert(id, index);
        index
    }
    fn edge(&mut self, from: usize, to: usize, directed: bool, execution: bool) {
        self.edges.push(Edge {
            from,
            to,
            directed,
            execution,
        });
    }
}

pub(super) fn method(m: &Method) -> Graph {
    let mut layout = Layout {
        method: m,
        nodes: vec![],
        edges: vec![],
        levels: vec![],
        next_y: BTreeMap::new(),
        values: BTreeMap::new(),
    };
    let mut operators = Vec::new();
    for &id in &m.inputs {
        if !layout.values.contains_key(&id) {
            layout.value(id, 0, 48.);
        }
    }
    for (i, op) in m.instructions.iter().enumerate() {
        let mut ports = Vec::new();
        let mut properties = Vec::new();
        let mut seen = BTreeSet::new();
        let mut parameters = 0;
        for arg in arguments(m, i) {
            if arg.role != ArgumentRole::Output
                && constant_tensor(m, arg.value)
                && !m.inputs.contains(&arg.value)
            {
                properties.push((arg.name, m.values[arg.value as usize].description.clone()));
            } else if parameter(m, &arg) {
                parameters += 1;
            } else if seen.insert((arg.value, arg.role as u8)) {
                ports.push(arg);
            }
        }
        let mut inputs = Vec::new();
        for arg in ports.iter().filter(|a| a.role != ArgumentRole::Output) {
            if let Some(&node) = layout.values.get(&arg.value) {
                inputs.push((node, arg.role));
            }
        }
        let level = inputs
            .iter()
            .map(|(n, _)| layout.levels[*n] + 1)
            .max()
            .unwrap_or(1);
        let y = inputs
            .iter()
            .map(|(n, _)| layout.nodes[*n].y)
            .fold(48., f32::max);
        let unknown = ports.iter().any(|a| a.role == ArgumentRole::Unknown);
        let input_count = ports
            .iter()
            .filter(|a| a.role == ArgumentRole::Input)
            .count();
        let output_count = ports
            .iter()
            .filter(|a| a.role == ArgumentRole::Output)
            .count();
        let subtitle = if unknown {
            format!("{} tensor/value ports · schema unavailable", ports.len())
        } else {
            format!("{input_count} inputs · {output_count} outputs · {parameters} parameters")
        };
        let property_count = properties.len();
        let operator = layout.push(
            Node {
                selection: Selection::Operator(i),
                x: 0.,
                y: 0.,
                w: 340.,
                h: 96. + property_count.min(3) as f32 * 22.,
                title: op
                    .name
                    .rsplit("::")
                    .next()
                    .unwrap_or(&op.name)
                    .split('.')
                    .next()
                    .unwrap_or(&op.name)
                    .into(),
                subtitle,
                properties: properties.into_iter().take(3).collect(),
                delegate: op.delegate.is_some(),
            },
            level,
            y,
        );
        operators.push(operator);
        let mut linked = BTreeSet::new();
        for (node, role) in inputs {
            if linked.insert(node) {
                layout.edge(node, operator, role != ArgumentRole::Unknown, false);
            }
        }
        for arg in ports.iter().filter(|a| a.role == ArgumentRole::Output) {
            // Track the latest producer, including in-place writes, without
            // inserting intermediate tensor nodes into the operator graph.
            layout.values.insert(arg.value, operator);
        }
        if i > 0
            && op.chain == m.instructions[i - 1].chain
            && (matches!(op.kind, 3..=5) || matches!(m.instructions[i - 1].kind, 3..=5))
        {
            layout.edge(operators[i - 1], operator, true, true);
        }
    }
    for (i, op) in m.instructions.iter().enumerate() {
        if let Some(target) = op.target
            && let Some(j) = m
                .instructions
                .iter()
                .position(|other| other.chain == op.chain && other.index == target as usize)
        {
            layout.edge(operators[i], operators[j], true, true);
        }
    }
    for &id in &m.outputs {
        let producer = layout.values.get(&id).copied();
        if m.instructions.is_empty() && producer.is_some() {
            continue;
        }
        let level = producer.map(|n| layout.levels[n] + 1).unwrap_or(1);
        let y = producer.map(|n| layout.nodes[n].y).unwrap_or(48.);
        let node = layout.value(id, level, y);
        if let Some(producer) = producer {
            layout.edge(producer, node, true, false);
        }
    }
    Graph {
        width: layout
            .nodes
            .iter()
            .map(|n| n.x + n.w + 32.)
            .fold(200., f32::max),
        height: layout
            .nodes
            .iter()
            .map(|n| n.y + n.h + 32.)
            .fold(200., f32::max),
        nodes: layout.nodes,
        edges: layout.edges,
    }
}
