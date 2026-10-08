-- Workflow on the Team page (2026-10-08, GA-49): a column decides who works its cards. A column holds an ordered list
-- of agents (column_agents), is Auto (its agents pick up its cards by themselves) or Manual (only Run starts one) and
-- links to the column its cards go to next (next_state_id). Label routing goes: routing_rules is dropped and labels
-- are plain tags. A removed column is kept (deleted_at), so old activity, runs and comments keep its name. The
-- organisation chart's branches are kept per team (teams.branches_json; NULL = the default branches).
-- Today's setup becomes the new one, so the board works as before on the first start: next columns follow 0.2.0's
-- moves, the agents routing sent to a column go on it, and To do, In progress and Testing columns that got an agent
-- turn Auto. An agent on manual wake-up isn't put on an Auto column, so it still only starts on Run.
ALTER TABLE workflow_states ADD COLUMN auto INTEGER NOT NULL DEFAULT 0;            -- 1: its agents pick up its cards
ALTER TABLE workflow_states ADD COLUMN next_state_id TEXT REFERENCES workflow_states(id);  -- where its cards go next
ALTER TABLE teams ADD COLUMN branches_json TEXT;   -- the org chart's branches [{key, name, roles}]; NULL = the defaults

CREATE TABLE column_agents (           -- the agents that work a column's cards, in order
  state_id TEXT NOT NULL REFERENCES workflow_states(id),
  actor_id TEXT NOT NULL REFERENCES actors(id),
  sort_key TEXT NOT NULL,
  created_at INTEGER NOT NULL,
  PRIMARY KEY (state_id, actor_id)
) STRICT;
CREATE INDEX column_agents_actor ON column_agents(actor_id);

-- Next columns, as 0.2.0 moved cards: To do → In progress → Testing → Review → Deploy (else Done), Deploy → Done.
-- Backlog, Done and Cancelled get none.
UPDATE workflow_states SET next_state_id = (
    SELECT n.id FROM workflow_states n
     WHERE n.team_id = workflow_states.team_id AND n.deleted_at IS NULL
       AND n.category = CASE workflow_states.category WHEN 'ready' THEN 'in_progress' WHEN 'in_progress' THEN 'testing'
                                                      WHEN 'testing' THEN 'review' WHEN 'deploy' THEN 'done' END
     ORDER BY n.sort_key LIMIT 1)
  WHERE deleted_at IS NULL AND category IN ('ready', 'in_progress', 'testing', 'deploy');
UPDATE workflow_states SET next_state_id = (
    SELECT n.id FROM workflow_states n
     WHERE n.team_id = workflow_states.team_id AND n.deleted_at IS NULL AND n.category IN ('deploy', 'done')
     ORDER BY n.category = 'done', n.sort_key LIMIT 1)
  WHERE deleted_at IS NULL AND category = 'review';

-- Who worked each column in 0.2.0 (agents that aren't archived): To do and In progress every builder (every role but
-- lead, qa and devops) and the roles a label rule sent there; Testing the column's owner role (qa); Deploy the DevOps
-- agents; and on To do, In progress and Testing the roles a column rule on it sent there.
CREATE TEMP TABLE ga49_workers AS
  SELECT s.id AS state_id, a.id AS actor_id, COALESCE(g.wakeup, 'manual') AS wakeup, m.created_at AS joined, a.name AS name
    FROM workflow_states s
    JOIN team_members m ON m.team_id = s.team_id AND m.deleted_at IS NULL
    JOIN actors a ON a.id = m.actor_id AND a.kind = 'agent' AND a.deleted_at IS NULL AND a.status <> 'archived'
    LEFT JOIN agent_configs g ON g.actor_id = a.id
   WHERE s.deleted_at IS NULL AND (
         (s.category IN ('ready', 'in_progress') AND (
             m.role_key NOT IN ('lead', 'qa', 'devops')
             OR EXISTS (SELECT 1 FROM routing_rules r WHERE r.team_id = s.team_id AND r.kind = 'label' AND r.enabled = 1
                          AND r.deleted_at IS NULL AND r.target_role = m.role_key)))
      OR (s.category = 'testing' AND (m.role_key = 'qa' OR m.role_key = s.owner_role))
      OR (s.category = 'deploy' AND m.role_key = 'devops')
      OR (s.category IN ('ready', 'in_progress', 'testing') AND EXISTS (
             SELECT 1 FROM routing_rules r WHERE r.team_id = s.team_id AND r.kind = 'column' AND r.match_state_id = s.id
                AND r.enabled = 1 AND r.deleted_at IS NULL AND r.target_role = m.role_key)));

-- Auto: To do, In progress and Testing columns that got an agent that started cards by itself (and have a next column).
UPDATE workflow_states SET auto = 1
  WHERE deleted_at IS NULL AND category IN ('ready', 'in_progress', 'testing') AND next_state_id IS NOT NULL
    AND EXISTS (SELECT 1 FROM ga49_workers w WHERE w.state_id = workflow_states.id AND w.wakeup <> 'manual');

-- The agents, in the order they joined the team; on an Auto column only those that didn't wait for Run.
INSERT INTO column_agents (state_id, actor_id, sort_key, created_at)
  SELECT w.state_id, w.actor_id, printf('a%04d', ROW_NUMBER() OVER (PARTITION BY w.state_id ORDER BY w.joined, w.name)),
         CAST(strftime('%s', 'now') AS INTEGER) * 1000
    FROM ga49_workers w JOIN workflow_states s ON s.id = w.state_id
   WHERE s.auto = 0 OR w.wakeup <> 'manual';

DROP TABLE ga49_workers;
DROP TABLE routing_rules;
